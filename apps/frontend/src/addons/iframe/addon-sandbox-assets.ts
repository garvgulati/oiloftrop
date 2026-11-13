export const ADDON_SANDBOX_RUNTIME_PROTOCOL_VERSION = 1;

export interface AddonSandboxRuntimeAssets {
  script: Blob;
  stylesheet: Blob;
}

let runtimeAssetsPromise: Promise<AddonSandboxRuntimeAssets> | undefined;

function runtimeAssetUrl(filename: string) {
  const basePath = import.meta.env.BASE_URL || "/";
  return new URL(
    `${basePath.replace(/\/?$/, "/")}__generated__/${filename}`,
    window.location.href,
  ).toString();
}

async function fetchRuntimeAsset(filename: string) {
  const response = await fetch(runtimeAssetUrl(filename), { cache: "no-cache" });
  if (!response.ok) {
    throw new Error(`${filename} returned HTTP ${response.status}`);
  }
  return response.blob();
}

export function loadAddonSandboxRuntimeAssets(): Promise<AddonSandboxRuntimeAssets> {
  if (runtimeAssetsPromise) {
    return runtimeAssetsPromise;
  }

  const pending = Promise.all([
    fetchRuntimeAsset("addon-sandbox-runtime.js"),
    fetchRuntimeAsset("addon-sandbox-runtime.css"),
  ]).then(([script, stylesheet]) => ({ script, stylesheet }));

  runtimeAssetsPromise = pending;
  void pending.catch(() => {
    if (runtimeAssetsPromise === pending) {
      runtimeAssetsPromise = undefined;
    }
  });
  return pending;
}

export function resetAddonSandboxRuntimeAssetsForTest() {
  runtimeAssetsPromise = undefined;
}
