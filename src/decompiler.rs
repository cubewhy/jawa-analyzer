use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::process::Child;

use crate::decompiler::backend::{cfr::CfrDecompiler, vineflower::VineflowerDecompiler};
use crate::lsp::request_cancellation::CancellationToken;

pub mod backend;
pub mod cache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecompilerType {
    Vineflower,
    Cfr,
}

impl DecompilerType {
    pub fn get_decompiler(&self) -> Box<dyn Decompiler + 'static> {
        match self {
            Self::Cfr => Box::new(CfrDecompiler),
            Self::Vineflower => Box::new(VineflowerDecompiler),
        }
    }
}

#[async_trait::async_trait]
pub trait Decompiler: Send + Sync {
    /// Perform the decompilation task
    /// class_path: The path to the .class files on disk or a temporary extraction path
    /// output_dir: The output directory for the decompiled results
    async fn decompile(
        &self,
        java_bin: &Path,
        decompiler_jar: &Path,
        class_data: &[u8],
        output_path: &Path,
        cancel: &CancellationToken,
    ) -> Result<()>;
}

pub(crate) async fn wait_with_output_or_cancel(
    mut child: Child,
    cancel: &CancellationToken,
) -> Result<std::process::Output> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut output = Vec::new();
        if let Some(mut stdout) = stdout {
            stdout.read_to_end(&mut output).await?;
        }
        Ok::<_, std::io::Error>(output)
    });
    let stderr_task = tokio::spawn(async move {
        let mut output = Vec::new();
        if let Some(mut stderr) = stderr {
            stderr.read_to_end(&mut output).await?;
        }
        Ok::<_, std::io::Error>(output)
    });

    tokio::select! {
        status = child.wait() => {
            let status = status?;
            let stdout = stdout_task.await??;
            let stderr = stderr_task.await??;
            Ok(std::process::Output { status, stdout, stderr })
        }
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            Err(anyhow!("request cancelled"))
        }
    }
}
