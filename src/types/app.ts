export type InstallKind = "deb" | "rpm" | "app_image" | "native" | "dev";

export interface AppInfo {
  buildRevision: string;
  releaseVersion?: string;
  installKind: InstallKind;
  installLabel: string;
  pipewireVersion?: string;
}

export type UpdateStatus =
  | "current"
  | "outdated"
  | "severely_outdated"
  | "unknown"
  | "checking"
  | "error"
  | "unsupported"
  | "dev_build";

export interface UpdateCheckResult {
  status: UpdateStatus;
  currentVersion: string;
  latestVersion?: string;
  releaseUrl?: string;
  downloadUrl?: string;
  canAutoInstall?: boolean;
  error?: string;
}
