import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

import {
  CONFIG_NAMESPACE,
  EXTENSION_CONFIG_KEYS,
  type DecompilerBackend,
  type ExtensionConfigKey,
  type ExtensionSettings,
} from "./config";

const COMMAND_SET_JDK_PATH = `${CONFIG_NAMESPACE}.setJdkPath`;
const COMMAND_SET_VINEFLOWER_PATH = `${CONFIG_NAMESPACE}.setVineflowerPath`;
const COMMAND_SET_CFR_PATH = `${CONFIG_NAMESPACE}.setCfrPath`;
const COMMAND_SELECT_DECOMPILER_BACKEND = `${CONFIG_NAMESPACE}.selectDecompilerBackend`;
const COMMAND_SET_SERVER_PATH = `${CONFIG_NAMESPACE}.setServerPath`;
const COMMAND_SHOW_MEMORY_STATUS = `${CONFIG_NAMESPACE}.showMemoryStatus`;
const COMMAND_CLEAR_SERVER_CACHES = `${CONFIG_NAMESPACE}.clearServerCaches`;

const SERVER_COMMAND_SHOW_MEMORY_STATUS = `${CONFIG_NAMESPACE}.server.showMemoryStatus`;
const SERVER_COMMAND_CLEAR_CACHES = `${CONFIG_NAMESPACE}.server.clearCaches`;

interface SelectableItem<T extends string> extends vscode.QuickPickItem {
  value: T;
}

type JdkSelectAction = "manual" | "browse" | "clear";

interface CommandDependencies {
  getSettings: () => ExtensionSettings;
  updateConfigurationValue: (key: ExtensionConfigKey, value: string) => Thenable<void>;
}

export function registerCommands(deps: CommandDependencies): vscode.Disposable[] {
  return [
    vscode.commands.registerCommand(COMMAND_SET_JDK_PATH, () => setJdkPath(deps)),
    vscode.commands.registerCommand(COMMAND_SET_VINEFLOWER_PATH, () => setVineflowerPath(deps)),
    vscode.commands.registerCommand(COMMAND_SET_CFR_PATH, () => setCfrPath(deps)),
    vscode.commands.registerCommand(
      COMMAND_SELECT_DECOMPILER_BACKEND,
      () => selectDecompilerBackend(deps),
    ),
    vscode.commands.registerCommand(COMMAND_SET_SERVER_PATH, () => setServerPath(deps)),
    vscode.commands.registerCommand(COMMAND_SHOW_MEMORY_STATUS, showMemoryStatus),
    vscode.commands.registerCommand(COMMAND_CLEAR_SERVER_CACHES, clearServerCaches),
  ];
}

function extractSummary(result: unknown): string {
  if (typeof result === "string") {
    return result;
  }
  if (
    result
    && typeof result === "object"
    && "summary" in result
    && typeof result.summary === "string"
  ) {
    return result.summary;
  }
  return "Language server command returned no summary.";
}

async function showMemoryStatus(): Promise<void> {
  try {
    const result = await vscode.commands.executeCommand(SERVER_COMMAND_SHOW_MEMORY_STATUS);
    const summary = extractSummary(result);
    const doc = await vscode.workspace.openTextDocument({
      language: "text",
      content: summary,
    });
    await vscode.window.showTextDocument(doc, { preview: false });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    await vscode.window.showErrorMessage(`Failed to fetch Java Analyzer memory status: ${detail}`);
  }
}

async function clearServerCaches(): Promise<void> {
  try {
    const result = await vscode.commands.executeCommand(SERVER_COMMAND_CLEAR_CACHES);
    const summary = extractSummary(result);
    await vscode.window.showInformationMessage(summary);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    await vscode.window.showErrorMessage(`Failed to clear Java Analyzer caches: ${detail}`);
  }
}

async function choosePath(
  title: string,
  currentValue: string,
  canSelectFiles: boolean,
  canSelectFolders: boolean,
): Promise<string | undefined> {
  const action = await vscode.window.showQuickPick(
    [
      {
        label: "$(folder-opened) Browse...",
        value: "browse",
      },
      {
        label: "$(edit) Enter path manually",
        value: "manual",
      },
      {
        label: "$(circle-slash) Clear",
        value: "clear",
      },
    ] satisfies Array<SelectableItem<"browse" | "manual" | "clear">>,
    {
      title,
      placeHolder: "Choose how to set this path",
    },
  );

  if (!action) {
    return undefined;
  }

  if (action.value === "clear") {
    return "";
  }

  if (action.value === "manual") {
    const manualPath = await vscode.window.showInputBox({
      title,
      prompt: "Enter path",
      value: currentValue,
      ignoreFocusOut: true,
    });
    if (manualPath === undefined) {
      return undefined;
    }
    return manualPath.trim();
  }

  const picked = await vscode.window.showOpenDialog({
    title,
    canSelectFiles,
    canSelectFolders,
    canSelectMany: false,
    defaultUri: currentValue ? vscode.Uri.file(currentValue) : undefined,
    openLabel: "Select",
  });

  if (!picked || picked.length === 0) {
    return undefined;
  }

  return picked[0].fsPath;
}

function expandHomePath(inputPath: string): string {
  const trimmed = inputPath.trim();
  if (trimmed === "~") {
    return os.homedir();
  }
  if (trimmed.startsWith(`~${path.sep}`)) {
    return path.join(os.homedir(), trimmed.slice(2));
  }
  if (path.sep === "\\" && trimmed.startsWith("~/")) {
    return path.join(os.homedir(), trimmed.slice(2));
  }
  return trimmed;
}

async function isValidJavaHome(candidatePath: string): Promise<boolean> {
  const resolvedPath = path.resolve(expandHomePath(candidatePath));
  let stat: Awaited<ReturnType<typeof fs.stat>>;
  try {
    stat = await fs.stat(resolvedPath);
  } catch {
    return false;
  }

  if (!stat.isDirectory()) {
    return false;
  }

  const javaBinary = path.join(
    resolvedPath,
    "bin",
    process.platform === "win32" ? "java.exe" : "java",
  );
  const modulesPath = path.join(resolvedPath, "lib", "modules");
  const rtJarPath = path.join(resolvedPath, "jre", "lib", "rt.jar");
  const altRtJarPath = path.join(resolvedPath, "lib", "rt.jar");

  const hasJavaBinary = await pathExists(javaBinary);
  const hasRuntimeLayout =
    (await pathExists(modulesPath))
    || (await pathExists(rtJarPath))
    || (await pathExists(altRtJarPath));

  return hasJavaBinary && hasRuntimeLayout;
}

async function pathExists(targetPath: string): Promise<boolean> {
  try {
    await fs.access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function discoverJdkPaths(): Promise<string[]> {
  const discovered = new Set<string>();
  const javaHome = process.env.JAVA_HOME?.trim();
  if (javaHome) {
    const expanded = path.resolve(expandHomePath(javaHome));
    if (await isValidJavaHome(expanded)) {
      discovered.add(expanded);
    }
  }

  const jdksDir = path.join(os.homedir(), ".jdks");
  let entries;
  try {
    entries = await fs.readdir(jdksDir, { withFileTypes: true });
  } catch {
    return [...discovered];
  }

  for (const entry of entries) {
    if (!entry.isDirectory() && !entry.isSymbolicLink()) {
      continue;
    }
    const candidate = path.resolve(jdksDir, entry.name.toString());
    if (await isValidJavaHome(candidate)) {
      discovered.add(candidate);
    }
  }

  return [...discovered].sort((a, b) => a.localeCompare(b));
}

async function promptManualJavaHome(title: string, currentValue: string): Promise<string | undefined> {
  const manualPath = await vscode.window.showInputBox({
    title,
    prompt: "Enter JAVA_HOME path",
    value: currentValue,
    ignoreFocusOut: true,
    validateInput: async (value) => {
      const trimmed = value.trim();
      if (!trimmed) {
        return "Path cannot be empty.";
      }
      const expanded = path.resolve(expandHomePath(trimmed));
      if (!(await isValidJavaHome(expanded))) {
        return "Invalid JAVA_HOME. Expected a JDK directory with bin/java and runtime libraries.";
      }
      return undefined;
    },
  });

  if (manualPath === undefined) {
    return undefined;
  }

  return path.resolve(expandHomePath(manualPath));
}

async function chooseJdkPath(currentValue: string): Promise<string | undefined> {
  const discoveredPaths = await discoverJdkPaths();
  const normalizedCurrent = currentValue ? path.resolve(expandHomePath(currentValue)) : "";

  const actions: Array<SelectableItem<JdkSelectAction>> = [
    {
      label: "$(edit) Enter JAVA_HOME manually",
      value: "manual",
    },
    {
      label: "$(folder-opened) Browse...",
      value: "browse",
    },
    {
      label: "$(circle-slash) Clear",
      value: "clear",
    },
  ];

  const discoveredItems = discoveredPaths.map((jdkPath) => ({
    label: "$(vm) " + jdkPath,
    description: jdkPath === normalizedCurrent ? "Current" : undefined,
    picked: jdkPath === normalizedCurrent,
    value: jdkPath,
  }));

  const picked = await vscode.window.showQuickPick(
    [...discoveredItems, ...actions],
    {
      title: "Set JDK Path",
      placeHolder: discoveredItems.length > 0
        ? "Select a detected JDK or choose another option"
        : "No detected JDKs. Choose how to set JAVA_HOME",
    },
  );

  if (!picked) {
    return undefined;
  }

  if (picked.value === "clear") {
    return "";
  }

  if (picked.value === "manual") {
    return promptManualJavaHome("Set JDK Path", normalizedCurrent);
  }

  if (picked.value === "browse") {
    const selected = await vscode.window.showOpenDialog({
      title: "Set JDK Path",
      canSelectFiles: false,
      canSelectFolders: true,
      canSelectMany: false,
      defaultUri: normalizedCurrent ? vscode.Uri.file(normalizedCurrent) : undefined,
      openLabel: "Select",
    });
    if (!selected || selected.length === 0) {
      return undefined;
    }

    const selectedPath = path.resolve(selected[0].fsPath);
    if (!(await isValidJavaHome(selectedPath))) {
      await vscode.window.showErrorMessage(
        "Invalid JAVA_HOME. Select a JDK directory that contains bin/java and runtime libraries.",
      );
      return undefined;
    }
    return selectedPath;
  }

  return picked.value;
}

async function setJdkPath(deps: CommandDependencies): Promise<void> {
  const currentPath = deps.getSettings().jdkPath;
  const selectedPath = await chooseJdkPath(currentPath);

  if (selectedPath === undefined) {
    return;
  }

  await deps.updateConfigurationValue(EXTENSION_CONFIG_KEYS.jdkPath, selectedPath);
}

async function setVineflowerPath(deps: CommandDependencies): Promise<void> {
  const currentPath = deps.getSettings().vineflowerPath;
  const selectedPath = await choosePath("Set Vineflower Path", currentPath, true, false);

  if (selectedPath === undefined) {
    return;
  }

  await deps.updateConfigurationValue(EXTENSION_CONFIG_KEYS.vineflowerPath, selectedPath);
}

async function setCfrPath(deps: CommandDependencies): Promise<void> {
  const currentPath = deps.getSettings().cfrPath;
  const selectedPath = await choosePath("Set CFR Path", currentPath, true, false);

  if (selectedPath === undefined) {
    return;
  }

  await deps.updateConfigurationValue(EXTENSION_CONFIG_KEYS.cfrPath, selectedPath);
}

async function setServerPath(deps: CommandDependencies): Promise<void> {
  const currentPath = deps.getSettings().serverPath;
  const selectedPath = await choosePath("Set Server Path", currentPath, true, false);

  if (selectedPath === undefined) {
    return;
  }

  await deps.updateConfigurationValue(EXTENSION_CONFIG_KEYS.serverPath, selectedPath);
}

async function selectDecompilerBackend(deps: CommandDependencies): Promise<void> {
  const currentBackend = deps.getSettings().decompilerBackend;
  const picked = await vscode.window.showQuickPick(
    [
      {
        label: "vineflower",
        detail: "Use the Vineflower decompiler backend",
        description: currentBackend === "vineflower" ? "Current" : undefined,
        picked: currentBackend === "vineflower",
        value: "vineflower",
      },
      {
        label: "cfr",
        detail: "Use the CFR decompiler backend",
        description: currentBackend === "cfr" ? "Current" : undefined,
        picked: currentBackend === "cfr",
        value: "cfr",
      },
    ] satisfies Array<SelectableItem<DecompilerBackend>>,
    {
      title: "Select Decompiler Backend",
      placeHolder: `Current: ${currentBackend}`,
    },
  );

  if (!picked) {
    return;
  }

  await deps.updateConfigurationValue(EXTENSION_CONFIG_KEYS.decompilerBackend, picked.value);
}
