import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import checkIcon from "lucide-static/icons/check.svg?raw";
import keyIcon from "lucide-static/icons/key-round.svg?raw";
import downloadIcon from "lucide-static/icons/download.svg?raw";
import folderIcon from "lucide-static/icons/folder.svg?raw";
import folderOpenIcon from "lucide-static/icons/folder-open.svg?raw";
import folderSearchIcon from "lucide-static/icons/folder-search.svg?raw";
import listIcon from "lucide-static/icons/list.svg?raw";
import packageIcon from "lucide-static/icons/package.svg?raw";
import playIcon from "lucide-static/icons/play.svg?raw";
import radarIcon from "lucide-static/icons/radar.svg?raw";
import refreshCwIcon from "lucide-static/icons/refresh-cw.svg?raw";
import shieldIcon from "lucide-static/icons/shield.svg?raw";
import trashIcon from "lucide-static/icons/trash-2.svg?raw";
import "./styles.css";

type SkyrimInstallation = {
  name: string;
  game_dir: string;
  data_dir: string | null;
  exe_path: string | null;
  skse_loader_path: string | null;
  steam_app_manifest: string | null;
  valid: boolean;
  issues: string[];
};

type SavesLocation = {
  name: string;
  path: string;
  exists: boolean;
  save_count: number;
};

type ModInstallResult = {
  name: string;
  source_url: string;
  archive_path: string;
  staging_dir: string;
  installed_to: string;
  copied_files: number;
  installed_mod_id: string;
  warnings: string[];
};

type InstalledFile = {
  path: string;
  existed_before: boolean;
};

type InstalledMod = {
  id: string;
  name: string;
  source_url: string;
  archive_path: string;
  staging_dir: string;
  game_dir: string;
  installed_to: string;
  installed_at: number;
  copied_files: InstalledFile[];
  warnings: string[];
};

type UninstallResult = {
  id: string;
  name: string;
  removed_files: number;
  skipped_files: number;
  archive_path: string;
};

type NexusAuthStatus = {
  configured: boolean;
  user_name: string | null;
  is_premium: boolean | null;
};

type InstallLog = {
  id?: string;
  timestamp?: number;
  action?: string;
  url: string;
  ok: boolean;
  message: string;
  mod_id?: string | null;
  mod_name?: string | null;
  result?: ModInstallResult;
};

type AppState = {
  installations: SkyrimInstallation[];
  selected: SkyrimInstallation | null;
  activeTab: "installer" | "mods";
  manualPath: string;
  modLinks: string;
  nexusApiKey: string;
  nexusStatus: NexusAuthStatus;
  installedMods: InstalledMod[];
  installLog: InstallLog[];
  savesLocations: SavesLocation[];
  status: string;
  busy: boolean;
};

const state: AppState = {
  installations: [],
  selected: null,
  activeTab: "installer",
  manualPath: "",
  modLinks: "",
  nexusApiKey: "",
  nexusStatus: {
    configured: false,
    user_name: null,
    is_premium: null
  },
  installedMods: [],
  installLog: [],
  savesLocations: [],
  status: "Ready",
  busy: false
};

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App root not found");
}

const root = app;

window.addEventListener("error", (event) => {
  root.innerHTML = `<main class="boot-error"><strong>Startup error</strong><p>${escapeHtml(event.message)}</p></main>`;
});

window.addEventListener("unhandledrejection", (event) => {
  const message = event.reason instanceof Error ? event.reason.message : String(event.reason);
  state.status = message;
  render();
});

const icons: Record<string, string> = {
  check: checkIcon,
  download: downloadIcon,
  key: keyIcon,
  folder: folderIcon,
  "folder-open": folderOpenIcon,
  "folder-search": folderSearchIcon,
  list: listIcon,
  package: packageIcon,
  play: playIcon,
  radar: radarIcon,
  "refresh-cw": refreshCwIcon,
  shield: shieldIcon,
  trash: trashIcon
};

function icon(name: string): string {
  return icons[name] ?? "";
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function renderInstallation(item: SkyrimInstallation, index: number): string {
  const selected = state.selected?.game_dir === item.game_dir;
  const statusClass = item.valid ? "ok" : "warn";
  const statusText = item.valid ? "Valid" : "Needs attention";
  const skseText = item.skse_loader_path ? "SKSE detected" : "SKSE not detected";

  return `
    <button class="install-row ${selected ? "selected" : ""}" data-select="${index}">
      <span class="install-main">
        <span class="install-title">${escapeHtml(item.name)}</span>
        <span class="install-path">${escapeHtml(item.game_dir)}</span>
      </span>
      <span class="install-meta">
        <span class="pill ${statusClass}">${statusText}</span>
        <span class="muted">${skseText}</span>
      </span>
    </button>
  `;
}

function renderSelected(item: SkyrimInstallation | null): string {
  if (!item) {
    return `
      <section class="panel empty">
        <div class="empty-icon">${icon("folder-search")}</div>
        <h2>No installation selected</h2>
        <p>Scan Steam libraries or choose the Skyrim Special Edition folder manually.</p>
      </section>
    `;
  }

  const issueList = item.issues.length
    ? item.issues.map((issue) => `<li>${escapeHtml(issue)}</li>`).join("")
    : "<li>No validation issues found.</li>";

  return `
    <section class="panel details">
      <div class="panel-head">
        <div>
          <h2>${escapeHtml(item.name)}</h2>
          <p>${escapeHtml(item.game_dir)}</p>
        </div>
        <span class="pill ${item.valid ? "ok" : "warn"}">${item.valid ? "Ready" : "Check setup"}</span>
      </div>

      <div class="facts">
        <div>
          <span>Executable</span>
          <strong>${escapeHtml(item.exe_path ?? "Missing")}</strong>
        </div>
        <div>
          <span>Data folder</span>
          <strong>${escapeHtml(item.data_dir ?? "Missing")}</strong>
        </div>
        <div>
          <span>SKSE loader</span>
          <strong>${escapeHtml(item.skse_loader_path ?? "Not installed")}</strong>
        </div>
        <div>
          <span>Steam manifest</span>
          <strong>${escapeHtml(item.steam_app_manifest ?? "Unknown")}</strong>
        </div>
      </div>

      <div class="issues">
        <h3>Validation</h3>
        <ul>${issueList}</ul>
      </div>

      ${
        state.savesLocations.length > 0
          ? `
      <div class="saves">
        <h3>Game Saves Locations</h3>
        <ul>
          ${state.savesLocations
            .map(
              (location) => `
            <li>
              <strong>${escapeHtml(location.name)}</strong>
              ${
                location.exists
                  ? `<span class="muted">${location.save_count} save${location.save_count === 1 ? "" : "s"}</span>`
                  : '<span class="muted">Not found</span>'
              }
              <span class="path">${escapeHtml(location.path)}</span>
            </li>
          `
            )
            .join("")}
        </ul>
      </div>
      `
          : ""
      }

      <div class="actions">
        <button id="run-game">${icon("play")}Run Skyrim</button>
        <button class="secondary" id="run-game-skse">${icon("play")}Run with SKSE</button>
        <button class="secondary" id="reveal-game">${icon("folder-open")}Reveal folder</button>
        <button class="secondary" id="rescan">${icon("refresh-cw")}Scan again</button>
      </div>
    </section>
  `;
}

function renderInstaller(): string {
  const logs = state.installLog.length
    ? state.installLog
        .map((entry) => {
          const warnings = entry.result?.warnings.length
            ? `<ul>${entry.result.warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul>`
            : "";
          return `
            <div class="log-row ${entry.ok ? "success" : "failure"}">
              <div>
                <strong>${escapeHtml(entry.result?.name ?? entry.mod_name ?? (entry.ok ? "Installed" : "Failed"))}</strong>
                <span>${escapeHtml(entry.url)}</span>
              </div>
              <p>${escapeHtml(entry.message)}</p>
              ${warnings}
            </div>
          `;
        })
        .join("")
    : `<div class="list-empty">No mod install attempts yet.</div>`;

  return `
    <section class="panel mod-panel">
      <div class="panel-head compact">
        <div>
          <h2>Mod Installer</h2>
          <p>Paste direct archive, Nexus page, or nxm links, one per line.</p>
        </div>
        <span class="pill ${state.selected ? "ok" : "warn"}">${state.selected ? "Target selected" : "No target"}</span>
      </div>

      <div class="nexus-auth">
        <div>
          <strong>Nexus Mods API</strong>
          <span>${
            state.nexusStatus.configured
              ? `Connected as ${escapeHtml(state.nexusStatus.user_name ?? "Nexus user")}${
                  state.nexusStatus.is_premium ? " - Premium" : ""
                }`
              : "Not connected"
          }</span>
        </div>
        <span class="api-status ${state.nexusStatus.configured ? "saved" : "missing"}">
          ${state.nexusStatus.configured ? `${icon("check")}Key saved` : `${icon("key")}No key`}
        </span>
        <div>
          <input id="nexus-api-key" value="${escapeHtml(state.nexusApiKey)}" type="password" placeholder="Paste Nexus API key" />
          <button class="secondary" id="save-nexus-key" title="Save Nexus API key" ${state.busy ? "disabled" : ""}>${icon("key")}</button>
        </div>
      </div>

      <label class="mod-links">
        <span>Download links</span>
        <textarea id="mod-links" placeholder="https://www.nexusmods.com/skyrimspecialedition/mods/123?tab=files&file_id=456&#10;nxm://skyrimspecialedition/mods/123/files/456?key=...&expires=...&#10;https://example.com/mod.7z">${escapeHtml(state.modLinks)}</textarea>
      </label>

      <div class="actions">
        <button id="install-mods" ${state.busy || !state.selected ? "disabled" : ""}>${icon("download")}Download and install</button>
        <button class="secondary" id="clear-log">${icon("package")}Clear log</button>
      </div>

      <div class="mod-log">
        ${logs}
      </div>
    </section>
  `;
}

function renderInstalledMods(): string {
  const mods = state.installedMods.length
    ? state.installedMods
        .map((mod) => {
          const installedDate = new Date(mod.installed_at * 1000).toLocaleString();
          const overwritten = mod.copied_files.filter((file) => file.existed_before).length;
          const warnings = mod.warnings.length
            ? `<ul>${mod.warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul>`
            : "";

          return `
            <div class="mod-card">
              <div class="mod-card-main">
                <div>
                  <strong>${escapeHtml(mod.name)}</strong>
                  <span>${escapeHtml(mod.source_url)}</span>
                </div>
                <button class="danger" data-uninstall="${escapeHtml(mod.id)}" ${state.busy ? "disabled" : ""}>${icon("trash")}Uninstall</button>
              </div>
              <div class="mod-card-facts">
                <span>${mod.copied_files.length} tracked files</span>
                <span>${overwritten} protected overwrite${overwritten === 1 ? "" : "s"}</span>
                <span>Installed ${escapeHtml(installedDate)}</span>
              </div>
              <p>Archive copy: ${escapeHtml(mod.archive_path)}</p>
              ${warnings}
            </div>
          `;
        })
        .join("")
    : `<div class="list-empty">No installed mods registered yet.</div>`;

  return `
    <section class="panel mod-panel">
      <div class="panel-head compact">
        <div>
          <h2>Installed Mods</h2>
          <p>Mods installed by this app, with local archive copies for reinstalling later.</p>
        </div>
        <span class="pill ${state.installedMods.length ? "ok" : "warn"}">${state.installedMods.length} tracked</span>
      </div>
      <div class="mods-list">
        ${mods}
      </div>
    </section>
  `;
}

function renderTopNav(): string {
  const apiConfigured = state.nexusStatus.configured;
  return `
    <div class="top-nav">
      <div class="tabs">
        <button class="tab ${state.activeTab === "installer" ? "active" : ""}" data-tab="installer">${icon("download")}Installer</button>
        <button class="tab ${state.activeTab === "mods" ? "active" : ""}" data-tab="mods">${icon("list")}Mods</button>
      </div>
      <span class="pill ${apiConfigured ? "ok" : "warn"}">${apiConfigured ? "API key registered" : "API key missing"}</span>
    </div>
  `;
}

function render(): void {
  root.innerHTML = `
    <main class="shell">
      <aside class="sidebar">
        <div class="brand">
          <div class="mark">${icon("shield")}</div>
          <div>
            <h1>Skyrim Auto Modder</h1>
            <p>Linux desktop mod workflow</p>
          </div>
        </div>

        <div class="toolbar">
          <button id="scan" ${state.busy ? "disabled" : ""}>${icon("radar")}Scan Steam</button>
          <button class="secondary" id="pick-folder" ${state.busy ? "disabled" : ""}>${icon("folder")}Choose</button>
        </div>

        <label class="manual">
          <span>Manual game folder</span>
          <div>
            <input id="manual-path" value="${escapeHtml(state.manualPath)}" placeholder="/path/to/Skyrim Special Edition" />
            <button id="validate-path" title="Validate folder" ${state.busy ? "disabled" : ""}>${icon("check")}</button>
          </div>
        </label>

        <div class="list-head">
          <span>Detected installs</span>
          <strong>${state.installations.length}</strong>
        </div>
        <div class="install-list">
          ${
            state.installations.length
              ? state.installations.map(renderInstallation).join("")
              : `<div class="list-empty">No Steam install scanned yet.</div>`
          }
        </div>
      </aside>

      <section class="content">
        ${renderTopNav()}
        <div class="content-grid">
          ${renderSelected(state.selected)}
          ${state.activeTab === "installer" ? renderInstaller() : renderInstalledMods()}
        </div>
        <footer>
          <span class="${state.busy ? "pulse" : ""}">${escapeHtml(state.status)}</span>
        </footer>
      </section>
    </main>
  `;

  bindEvents();
}

async function withBusy(status: string, task: () => Promise<void>): Promise<void> {
  state.busy = true;
  state.status = status;
  render();

  try {
    await task();
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
  } finally {
    state.busy = false;
    render();
  }
}

async function scan(): Promise<void> {
  await withBusy("Scanning Steam libraries...", async () => {
    const installations = await invoke<SkyrimInstallation[]>("scan_skyrim_installations");
    state.installations = installations;
    state.selected = installations[0] ?? null;
    if (state.selected) {
      await loadSavesLocations(state.selected.game_dir);
    }
    state.status = installations.length
      ? `Found ${installations.length} installation${installations.length === 1 ? "" : "s"}.`
      : "No Skyrim Special Edition install found in Steam libraries.";
  });
}

async function validateManualPath(path: string): Promise<void> {
  const trimmed = path.trim();
  if (!trimmed) {
    state.status = "Enter or choose a Skyrim Special Edition folder first.";
    render();
    return;
  }

  await withBusy("Validating folder...", async () => {
    const installation = await invoke<SkyrimInstallation>("validate_skyrim_path", { path: trimmed });
    const existingIndex = state.installations.findIndex((item) => item.game_dir === installation.game_dir);
    if (existingIndex >= 0) {
      state.installations[existingIndex] = installation;
    } else {
      state.installations = [installation, ...state.installations];
    }
    state.selected = installation;
    await loadSavesLocations(installation.game_dir);
    state.status = installation.valid ? "Folder validated." : "Folder found, but needs attention.";
  });
}

async function installMods(): Promise<void> {
  if (!state.selected) {
    state.status = "Choose a Skyrim installation first.";
    render();
    return;
  }

  const links = state.modLinks
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  if (!links.length) {
    state.status = "Paste at least one mod archive link.";
    render();
    return;
  }

  await withBusy(`Installing ${links.length} mod${links.length === 1 ? "" : "s"}...`, async () => {
    await installLinks(links);
  });
}

async function installLinks(links: string[]): Promise<void> {
  for (const [index, url] of links.entries()) {
    state.status = `Installing ${index + 1}/${links.length}: ${url}`;
    render();

    try {
      const result = await invoke<ModInstallResult>("install_mod_from_url", {
        url,
        gameDir: state.selected?.game_dir
      });
      state.installLog = [
          {
            url,
            ok: true,
            message: `Copied ${result.copied_files} file${result.copied_files === 1 ? "" : "s"} into Data.`,
            mod_id: result.installed_mod_id,
            mod_name: result.name,
            result
          },
        ...state.installLog
      ];
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        await invoke("append_install_log_entry", {
          action: "install",
          url,
          ok: false,
          message
        });
        state.installLog = [
          {
            url,
            ok: false,
            message
          },
          ...state.installLog
        ];
      }
  }

  await loadInstalledMods(false);
  state.status = "Install queue finished.";
}

async function installDeepLinks(urls: string[]): Promise<void> {
  const nxmLinks = urls.map((url) => url.trim()).filter((url) => url.startsWith("nxm://"));
  if (!nxmLinks.length) {
    return;
  }

  const existingLinks = state.modLinks
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  const newLinks = nxmLinks.filter((url) => !existingLinks.includes(url));
  if (newLinks.length) {
    state.modLinks = [...existingLinks, ...newLinks].join("\n");
  }

  if (!state.selected) {
    state.status = "Received Nexus download link, but no Skyrim installation is selected.";
    render();
    return;
  }

  await withBusy(`Installing ${nxmLinks.length} Nexus download${nxmLinks.length === 1 ? "" : "s"}...`, async () => {
    await installLinks(nxmLinks);
  });
}

async function setupDeepLinks(): Promise<void> {
  try {
    const startupUrls = await getCurrent();
    if (startupUrls?.length) {
      await installDeepLinks(startupUrls);
    }

    await onOpenUrl((urls) => {
      void installDeepLinks(urls);
    });
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
    render();
  }
}

async function saveNexusKey(): Promise<void> {
  const apiKey = state.nexusApiKey.trim();
  if (!apiKey) {
    state.status = "Paste a Nexus Mods API key first.";
    render();
    return;
  }

  await withBusy("Validating Nexus API key...", async () => {
    state.nexusStatus = await invoke<NexusAuthStatus>("save_nexus_api_key", { apiKey });
    state.nexusApiKey = "";
    state.status = `Nexus connected as ${state.nexusStatus.user_name ?? "user"}.`;
  });
}

async function loadNexusStatus(): Promise<void> {
  try {
    state.nexusStatus = await invoke<NexusAuthStatus>("get_nexus_auth_status");
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
  }
  render();
}

async function loadInstalledMods(shouldRender = true): Promise<void> {
  try {
    state.installedMods = await invoke<InstalledMod[]>("list_installed_mods");
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
  }
  if (shouldRender) {
    render();
  }
}

async function loadInstallLogs(shouldRender = true): Promise<void> {
  try {
    state.installLog = await invoke<InstallLog[]>("list_install_logs");
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
  }
  if (shouldRender) {
    render();
  }
}

async function loadSavesLocations(gameDir: string): Promise<void> {
  try {
    state.savesLocations = await invoke<SavesLocation[]>("get_saves_locations", { game_dir: gameDir });
  } catch (error) {
    state.status = error instanceof Error ? error.message : String(error);
    state.savesLocations = [];
  }
}

async function uninstallInstalledMod(id: string): Promise<void> {
  const mod = state.installedMods.find((item) => item.id === id);
  if (!mod) {
    state.status = "Installed mod was not found.";
    render();
    return;
  }

  const confirmed = window.confirm(`Uninstall ${mod.name}? Files that existed before this install will be preserved.`);
  if (!confirmed) {
    return;
  }

  await withBusy(`Uninstalling ${mod.name}...`, async () => {
    const result = await invoke<UninstallResult>("uninstall_mod", { id });
    await loadInstalledMods(false);
    await loadInstallLogs(false);
    state.status = `Uninstalled ${result.name}. Removed ${result.removed_files} file${result.removed_files === 1 ? "" : "s"}; preserved ${result.skipped_files}. Archive kept at ${result.archive_path}.`;
  });
}

async function runGame(gameDir: string, useSKSE: boolean): Promise<void> {
  await withBusy(useSKSE ? "Starting Skyrim with SKSE..." : "Starting Skyrim...", async () => {
    const message = await invoke<string>("run_skyrim", { 
      game_dir: gameDir, 
      use_skse: useSKSE 
    });
    state.status = message;
  });
}

function bindEvents(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      const tab = button.dataset.tab;
      if (tab === "installer" || tab === "mods") {
        state.activeTab = tab;
        render();
      }
    });
  });

  document.querySelector<HTMLButtonElement>("#scan")?.addEventListener("click", () => {
    void scan();
  });

  document.querySelector<HTMLButtonElement>("#rescan")?.addEventListener("click", () => {
    void scan();
  });

  document.querySelector<HTMLInputElement>("#manual-path")?.addEventListener("input", (event) => {
    state.manualPath = (event.target as HTMLInputElement).value;
  });

  document.querySelector<HTMLButtonElement>("#validate-path")?.addEventListener("click", () => {
    void validateManualPath(state.manualPath);
  });

  document.querySelector<HTMLTextAreaElement>("#mod-links")?.addEventListener("input", (event) => {
    state.modLinks = (event.target as HTMLTextAreaElement).value;
  });

  document.querySelector<HTMLInputElement>("#nexus-api-key")?.addEventListener("input", (event) => {
    state.nexusApiKey = (event.target as HTMLInputElement).value;
  });

  document.querySelector<HTMLButtonElement>("#save-nexus-key")?.addEventListener("click", () => {
    void saveNexusKey();
  });

  document.querySelector<HTMLButtonElement>("#install-mods")?.addEventListener("click", () => {
    void installMods();
  });

  document.querySelector<HTMLButtonElement>("#clear-log")?.addEventListener("click", () => {
    void withBusy("Clearing install log...", async () => {
      await invoke("clear_install_logs");
      state.installLog = [];
      state.status = "Install log cleared.";
    });
  });

  document.querySelector<HTMLButtonElement>("#pick-folder")?.addEventListener("click", async () => {
    const folder = await open({ directory: true, multiple: false, title: "Choose Skyrim Special Edition folder" });
    if (typeof folder === "string") {
      state.manualPath = folder;
      await validateManualPath(folder);
    }
  });

  document.querySelectorAll<HTMLButtonElement>("[data-select]").forEach((button) => {
    button.addEventListener("click", async () => {
      const index = Number(button.dataset.select);
      state.selected = state.installations[index] ?? null;
      if (state.selected) {
        await loadSavesLocations(state.selected.game_dir);
      }
      state.status = state.selected ? `Selected ${state.selected.name}.` : "Selection cleared.";
      render();
    });
  });

  document.querySelector<HTMLButtonElement>("#reveal-game")?.addEventListener("click", async () => {
    if (!state.selected) {
      return;
    }
    await revealItemInDir(state.selected.game_dir);
  });

  document.querySelector<HTMLButtonElement>("#run-game")?.addEventListener("click", async () => {
    if (!state.selected) {
      return;
    }
    await runGame(state.selected.game_dir, false);
  });

  document.querySelector<HTMLButtonElement>("#run-game-skse")?.addEventListener("click", async () => {
    if (!state.selected) {
      return;
    }
    await runGame(state.selected.game_dir, true);
  });

  document.querySelectorAll<HTMLButtonElement>("[data-uninstall]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.uninstall;
      if (id) {
        void uninstallInstalledMod(id);
      }
    });
  });
}

async function initialize(): Promise<void> {
  render();
  await Promise.allSettled([loadNexusStatus(), loadInstalledMods(false), loadInstallLogs(false), scan()]);
  render();
  void setupDeepLinks();
}

void initialize();
