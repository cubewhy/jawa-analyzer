use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use notify::event::{ModifyKind, RenameMode};
use notify::{ErrorKind as NotifyErrorKind, Event, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

use crate::index::codebase::{collect_source_files_for_root, should_index_source_path};
use crate::workspace::{FilesystemApplySummary, FilesystemChange, WatchedSourceRoot, Workspace};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(900);
const DEFAULT_NOTIFY_DEBOUNCE: Duration = Duration::from_millis(250);
const FULL_REINDEX_SETTLE_POLL: Duration = Duration::from_millis(50);

type SourceDebouncer = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

type WatchSnapshot = HashMap<PathBuf, FileFingerprint>;

#[derive(Debug, Default)]
struct EventBatch {
    changes: Vec<FilesystemChange>,
    needs_rescan: bool,
}

pub struct SourceWatchService {
    task: JoinHandle<()>,
}

impl SourceWatchService {
    pub fn new(workspace: Arc<Workspace>, client: Client) -> Self {
        let roots_rx = workspace.subscribe_watched_source_roots();
        let task = match start_notify_loop(Arc::clone(&workspace), client.clone(), roots_rx) {
            Ok(task) => task,
            Err(error) => {
                tracing::warn!(%error, "notify watcher unavailable, falling back to polling");
                let fallback_roots_rx = workspace.subscribe_watched_source_roots();
                tokio::spawn(run_polling_fallback_loop(
                    workspace,
                    client,
                    fallback_roots_rx,
                ))
            }
        };

        Self { task }
    }

    pub fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for SourceWatchService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn start_notify_loop(
    workspace: Arc<Workspace>,
    client: Client,
    roots_rx: watch::Receiver<Vec<WatchedSourceRoot>>,
) -> notify::Result<JoinHandle<()>> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let debouncer = new_debouncer(
        DEFAULT_NOTIFY_DEBOUNCE,
        None,
        move |result: DebounceEventResult| {
            let _ = event_tx.send(result);
        },
    )?;

    Ok(tokio::spawn(run_notify_loop(
        workspace, client, roots_rx, debouncer, event_rx,
    )))
}

async fn run_notify_loop(
    workspace: Arc<Workspace>,
    client: Client,
    mut roots_rx: watch::Receiver<Vec<WatchedSourceRoot>>,
    mut debouncer: SourceDebouncer,
    mut event_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
) {
    let mut desired_roots = roots_rx.borrow().clone();
    let mut watched_paths = HashSet::new();

    if let Err(error) = sync_watched_paths(&mut debouncer, &mut watched_paths, &desired_roots) {
        log_notify_error(
            &client,
            format!("Failed to configure source watcher: {error:#}"),
        )
        .await;
    }

    loop {
        tokio::select! {
            changed = roots_rx.changed() => {
                if changed.is_err() {
                    break;
                }

                let next_roots = roots_rx.borrow().clone();
                if let Err(error) = sync_watched_paths(&mut debouncer, &mut watched_paths, &next_roots) {
                    log_notify_error(
                        &client,
                        format!("Failed to update source watcher roots: {error:#}"),
                    ).await;
                }
                desired_roots = next_roots;
            }
            Some(result) = event_rx.recv() => {
                match result {
                    Ok(events) => {
                        if let Err(error) = handle_debounced_events(
                            Arc::clone(&workspace),
                            client.clone(),
                            desired_roots.clone(),
                            events,
                        ).await {
                            log_notify_error(
                                &client,
                                format!("Source watcher apply failed: {error:#}"),
                            ).await;
                        }
                    }
                    Err(errors) => {
                        for error in &errors {
                            tracing::warn!(%error, "notify watcher reported an error");
                        }
                        if let Err(error) = rescan_roots_after_notify_error(
                            Arc::clone(&workspace),
                            client.clone(),
                            desired_roots.clone(),
                        ).await {
                            log_notify_error(
                                &client,
                                format!("Source watcher recovery failed: {error:#}"),
                            ).await;
                        }
                    }
                }
            }
            else => break,
        }
    }
}

async fn run_polling_fallback_loop(
    workspace: Arc<Workspace>,
    client: Client,
    mut roots_rx: watch::Receiver<Vec<WatchedSourceRoot>>,
) {
    let mut interval = tokio::time::interval(DEFAULT_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut current_roots = roots_rx.borrow().clone();
    let mut previous_snapshot = match tokio::task::spawn_blocking({
        let roots = current_roots.clone();
        move || snapshot_roots(&roots)
    })
    .await
    {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            tracing::error!(%error, "polling watcher snapshot task panicked");
            None
        }
    };

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let snapshot = match tokio::task::spawn_blocking({
                    let roots = current_roots.clone();
                    move || snapshot_roots(&roots)
                }).await {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        tracing::error!(%error, "polling watcher snapshot task panicked");
                        continue;
                    }
                };

                let Some(previous) = previous_snapshot.as_ref() else {
                    previous_snapshot = Some(snapshot);
                    continue;
                };

                let changes = diff_snapshots(previous, &snapshot);
                if changes.is_empty() {
                    previous_snapshot = Some(snapshot);
                    continue;
                }

                match apply_workspace_changes(
                    Arc::clone(&workspace),
                    client.clone(),
                    current_roots.clone(),
                    changes,
                    false,
                ).await {
                    Ok(_) => {
                        previous_snapshot = Some(snapshot);
                    }
                    Err(error) => {
                        log_notify_error(
                            &client,
                            format!("Polling source watcher apply failed: {error:#}"),
                        ).await;
                    }
                }
            }
            changed = roots_rx.changed() => {
                if changed.is_err() {
                    break;
                }

                current_roots = roots_rx.borrow().clone();
                previous_snapshot = match tokio::task::spawn_blocking({
                    let roots = current_roots.clone();
                    move || snapshot_roots(&roots)
                }).await {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        tracing::error!(%error, "polling watcher root-refresh snapshot task panicked");
                        previous_snapshot
                    }
                };
            }
        }
    }
}

async fn handle_debounced_events(
    workspace: Arc<Workspace>,
    client: Client,
    roots: Vec<WatchedSourceRoot>,
    events: Vec<DebouncedEvent>,
) -> anyhow::Result<()> {
    let batch = collect_event_batch(&roots, events);
    if batch.changes.is_empty() && !batch.needs_rescan {
        return Ok(());
    }

    apply_workspace_changes(workspace, client, roots, batch.changes, batch.needs_rescan).await
}

async fn rescan_roots_after_notify_error(
    workspace: Arc<Workspace>,
    client: Client,
    roots: Vec<WatchedSourceRoot>,
) -> anyhow::Result<()> {
    apply_workspace_changes(workspace, client, roots, Vec::new(), true).await
}

async fn apply_workspace_changes(
    workspace: Arc<Workspace>,
    client: Client,
    roots: Vec<WatchedSourceRoot>,
    changes: Vec<FilesystemChange>,
    needs_rescan: bool,
) -> anyhow::Result<()> {
    if roots.is_empty() {
        return Ok(());
    }

    wait_for_full_reindex(&workspace).await;
    let reindex_serial_before = workspace.full_reindex_serial();

    let workspace_for_apply = Arc::clone(&workspace);
    let roots_for_apply = roots.clone();
    let apply_result = tokio::task::spawn_blocking(move || {
        if needs_rescan {
            workspace_for_apply.rescan_watched_roots_blocking(roots_for_apply)
        } else {
            workspace_for_apply.apply_filesystem_changes_blocking(changes)
        }
    })
    .await?;

    let summary = apply_result?;

    if workspace.full_reindex_serial() != reindex_serial_before {
        wait_for_full_reindex(&workspace).await;
        let workspace_for_rescan = Arc::clone(&workspace);
        let roots_for_rescan = roots.clone();
        let rescan_result = tokio::task::spawn_blocking(move || {
            workspace_for_rescan.rescan_watched_roots_blocking(roots_for_rescan)
        })
        .await?;
        let rescan_summary = rescan_result?;
        refresh_semantic_tokens_if_needed(&client, rescan_summary).await;
        return Ok(());
    }

    refresh_semantic_tokens_if_needed(&client, summary).await;
    Ok(())
}

async fn refresh_semantic_tokens_if_needed(client: &Client, summary: FilesystemApplySummary) {
    if summary.applied > 0 {
        client.semantic_tokens_refresh().await.ok();
        tracing::debug!(
            applied = summary.applied,
            removed = summary.removed,
            skipped_open_documents = summary.skipped_open_documents,
            "source watcher applied filesystem changes"
        );
    }
}

async fn wait_for_full_reindex(workspace: &Workspace) {
    while workspace.full_reindex_in_progress() {
        tokio::time::sleep(FULL_REINDEX_SETTLE_POLL).await;
    }
}

fn sync_watched_paths(
    debouncer: &mut SourceDebouncer,
    watched_paths: &mut HashSet<PathBuf>,
    roots: &[WatchedSourceRoot],
) -> notify::Result<()> {
    let desired_paths = roots
        .iter()
        .filter_map(|root| nearest_existing_watch_path(&root.path))
        .collect::<HashSet<_>>();

    let removals = watched_paths
        .difference(&desired_paths)
        .cloned()
        .collect::<Vec<_>>();
    let additions = desired_paths
        .difference(watched_paths)
        .cloned()
        .collect::<Vec<_>>();

    for path in removals {
        if let Err(error) = debouncer.unwatch(&path)
            && !matches!(error.kind, NotifyErrorKind::WatchNotFound)
        {
            return Err(error);
        }
    }

    for path in additions {
        debouncer.watch(&path, RecursiveMode::Recursive)?;
    }

    *watched_paths = desired_paths;
    Ok(())
}

fn nearest_existing_watch_path(root: &Path) -> Option<PathBuf> {
    root.ancestors()
        .find(|candidate| candidate.exists())
        .map(Path::to_path_buf)
}

fn collect_event_batch(roots: &[WatchedSourceRoot], events: Vec<DebouncedEvent>) -> EventBatch {
    let mut batch = EventBatch::default();
    let mut seen = HashSet::new();

    for event in events {
        if event.need_rescan() {
            batch.needs_rescan = true;
            continue;
        }

        let mapped = map_notify_event(roots, &event.event);
        batch.needs_rescan |= mapped.needs_rescan;
        for change in mapped.changes {
            if seen.insert((change.kind, change.path.clone())) {
                batch.changes.push(change);
            }
        }
    }

    batch
}

fn map_notify_event(roots: &[WatchedSourceRoot], event: &Event) -> EventBatch {
    match &event.kind {
        EventKind::Create(_) => {
            map_path_kinds(roots, &event.paths, FilesystemChange::upsert, false)
        }
        EventKind::Modify(ModifyKind::Name(mode)) => map_rename_event(roots, &event.paths, *mode),
        EventKind::Modify(_) => {
            map_path_kinds(roots, &event.paths, FilesystemChange::upsert, false)
        }
        EventKind::Remove(_) => map_path_kinds(roots, &event.paths, FilesystemChange::remove, true),
        EventKind::Access(_) => EventBatch::default(),
        EventKind::Any | EventKind::Other => EventBatch {
            needs_rescan: event
                .paths
                .iter()
                .any(|path| path_within_any_root(path, roots)),
            changes: Vec::new(),
        },
    }
}

fn map_rename_event(
    roots: &[WatchedSourceRoot],
    paths: &[PathBuf],
    mode: RenameMode,
) -> EventBatch {
    match mode {
        RenameMode::Both if paths.len() >= 2 => {
            let mut batch = EventBatch::default();
            batch.changes.extend(path_to_change(
                roots,
                &paths[0],
                FilesystemChange::remove,
                true,
            ));
            batch.changes.extend(path_to_change(
                roots,
                &paths[1],
                FilesystemChange::upsert,
                false,
            ));
            if batch.changes.is_empty()
                && (path_within_any_root(&paths[0], roots)
                    || path_within_any_root(&paths[1], roots))
            {
                batch.needs_rescan = true;
            }
            batch
        }
        RenameMode::Both => EventBatch {
            needs_rescan: paths.iter().any(|path| path_within_any_root(path, roots)),
            changes: Vec::new(),
        },
        RenameMode::From => map_path_kinds(roots, paths, FilesystemChange::remove, true),
        RenameMode::To => map_path_kinds(roots, paths, FilesystemChange::upsert, false),
        RenameMode::Any | RenameMode::Other => EventBatch {
            needs_rescan: paths.iter().any(|path| path_within_any_root(path, roots)),
            changes: Vec::new(),
        },
    }
}

fn map_path_kinds(
    roots: &[WatchedSourceRoot],
    paths: &[PathBuf],
    build_change: impl Fn(PathBuf) -> FilesystemChange,
    rescan_for_non_source: bool,
) -> EventBatch {
    let mut batch = EventBatch::default();
    for path in paths {
        let changes = path_to_change(roots, path, &build_change, rescan_for_non_source);
        if changes.is_empty() && rescan_for_non_source && path_within_any_root(path, roots) {
            batch.needs_rescan = true;
        }
        batch.changes.extend(changes);
    }
    batch
}

fn path_to_change(
    roots: &[WatchedSourceRoot],
    path: &Path,
    build_change: impl Fn(PathBuf) -> FilesystemChange,
    rescan_for_non_source: bool,
) -> Vec<FilesystemChange> {
    let Some(root) = matching_root(path, roots) else {
        return Vec::new();
    };

    if should_index_source_path(path, root.scan_mode) {
        return vec![build_change(path.to_path_buf())];
    }

    if rescan_for_non_source && path.extension().is_none() {
        return Vec::new();
    }

    Vec::new()
}

fn matching_root<'a>(path: &Path, roots: &'a [WatchedSourceRoot]) -> Option<&'a WatchedSourceRoot> {
    roots
        .iter()
        .filter(|root| path.starts_with(&root.path))
        .max_by_key(|root| root.path.components().count())
}

fn path_within_any_root(path: &Path, roots: &[WatchedSourceRoot]) -> bool {
    matching_root(path, roots).is_some()
}

fn snapshot_roots(roots: &[WatchedSourceRoot]) -> WatchSnapshot {
    let mut snapshot = HashMap::new();
    for root in roots {
        for path in collect_source_files_for_root(root.path.clone(), root.scan_mode) {
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            snapshot.insert(
                path,
                FileFingerprint {
                    len: metadata.len(),
                    modified: metadata.modified().ok(),
                },
            );
        }
    }
    snapshot
}

fn diff_snapshots(previous: &WatchSnapshot, current: &WatchSnapshot) -> Vec<FilesystemChange> {
    let mut removed = previous
        .keys()
        .filter(|path| !current.contains_key(*path))
        .cloned()
        .collect::<Vec<_>>();
    removed.sort();

    let mut upserts = current
        .iter()
        .filter_map(|(path, fingerprint)| {
            if previous.get(path) == Some(fingerprint) {
                None
            } else {
                Some(path.clone())
            }
        })
        .collect::<Vec<_>>();
    upserts.sort();

    removed
        .into_iter()
        .map(FilesystemChange::remove)
        .chain(upserts.into_iter().map(FilesystemChange::upsert))
        .collect()
}

async fn log_notify_error(client: &Client, message: String) {
    tracing::warn!("{message}");
    client.log_message(MessageType::WARNING, message).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::EventKind;
    use notify::event::{CreateKind, EventAttributes, RemoveKind};
    use std::time::Instant;

    fn root(path: &str) -> WatchedSourceRoot {
        WatchedSourceRoot {
            path: PathBuf::from(path),
            scan_mode: crate::index::codebase::SourceScanMode::Default,
        }
    }

    fn event(kind: EventKind, paths: Vec<&str>) -> DebouncedEvent {
        DebouncedEvent::new(
            Event {
                kind,
                paths: paths.into_iter().map(PathBuf::from).collect(),
                attrs: EventAttributes::default(),
            },
            Instant::now(),
        )
    }

    fn fingerprint(len: u64, modified: u64) -> FileFingerprint {
        FileFingerprint {
            len,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(modified)),
        }
    }

    #[test]
    fn diff_orders_remove_before_upsert() {
        let previous = HashMap::from([
            (PathBuf::from("/tmp/A.java"), fingerprint(1, 1)),
            (PathBuf::from("/tmp/B.java"), fingerprint(1, 1)),
        ]);
        let current = HashMap::from([
            (PathBuf::from("/tmp/B.java"), fingerprint(2, 2)),
            (PathBuf::from("/tmp/C.java"), fingerprint(1, 1)),
        ]);

        let actual = diff_snapshots(&previous, &current);
        assert_eq!(
            actual,
            vec![
                FilesystemChange::remove(PathBuf::from("/tmp/A.java")),
                FilesystemChange::upsert(PathBuf::from("/tmp/B.java")),
                FilesystemChange::upsert(PathBuf::from("/tmp/C.java")),
            ]
        );
    }

    #[test]
    fn maps_source_create_to_upsert() {
        let actual = collect_event_batch(
            &[root("/workspace/src")],
            vec![event(
                EventKind::Create(CreateKind::File),
                vec!["/workspace/src/Foo.java"],
            )],
        );

        assert_eq!(
            actual.changes,
            vec![FilesystemChange::upsert(PathBuf::from(
                "/workspace/src/Foo.java"
            ))]
        );
        assert!(!actual.needs_rescan);
    }

    #[test]
    fn maps_rename_to_remove_and_upsert() {
        let actual = collect_event_batch(
            &[root("/workspace/src")],
            vec![event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                vec!["/workspace/src/Foo.java", "/workspace/src/Bar.java"],
            )],
        );

        assert_eq!(
            actual.changes,
            vec![
                FilesystemChange::remove(PathBuf::from("/workspace/src/Foo.java")),
                FilesystemChange::upsert(PathBuf::from("/workspace/src/Bar.java")),
            ]
        );
        assert!(!actual.needs_rescan);
    }

    #[test]
    fn directory_remove_requests_rescan() {
        let actual = collect_event_batch(
            &[root("/workspace/src")],
            vec![event(
                EventKind::Remove(RemoveKind::Folder),
                vec!["/workspace/src/com"],
            )],
        );

        assert!(actual.changes.is_empty());
        assert!(actual.needs_rescan);
    }
}
