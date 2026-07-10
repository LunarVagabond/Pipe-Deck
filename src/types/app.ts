export type InstallKind = "flatpak" | "deb" | "rpm" | "app_image" | "native" | "dev";

export interface AppInfo {
  buildRevision: string;
  releaseVersion?: string;
  installKind: InstallKind;
  backgroundRestoreSupported: boolean;
  installLabel: string;
}

export type UpdateStatus = "current" | "outdated" | "severely_outdated" | "unknown" | "checking";

export interface UpdateCheckResult {
  status: UpdateStatus;
  currentVersion: string;
  latestVersion?: string;
  releaseUrl?: string;
  error?: string;
}
