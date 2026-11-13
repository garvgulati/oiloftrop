export interface SandboxAddonAsset {
  id: string;
  path: string;
  mimeType: string;
  size: number;
}

export function normalizeAddonAssetPath(path: string) {
  if (!path || path.includes("\0")) {
    throw new Error("Packaged addon asset path is invalid");
  }

  const parts = path.replace(/\\/g, "/").replace(/^\/+/, "").split("/");
  const normalized: string[] = [];
  for (const part of parts) {
    if (!part || part === ".") {
      continue;
    }
    if (part === "..") {
      if (normalized.length === 0) {
        throw new Error(`Packaged addon asset path '${path}' escapes the package root`);
      }
      normalized.pop();
      continue;
    }
    normalized.push(part);
  }

  if (normalized.length === 0) {
    throw new Error("Packaged addon asset path is invalid");
  }
  return normalized.join("/");
}

export function resolveAddonAssetPath(path: string, importerPath: string) {
  if (path.startsWith("/")) {
    return normalizeAddonAssetPath(path);
  }
  const normalizedImporter = normalizeAddonAssetPath(importerPath);
  const separator = normalizedImporter.lastIndexOf("/");
  const directory = separator === -1 ? "" : normalizedImporter.slice(0, separator);
  return normalizeAddonAssetPath(directory ? `${directory}/${path}` : path);
}

export class SandboxAddonAssetRegistry {
  private readonly assetsByPath = new Map<string, SandboxAddonAsset>();
  private readonly blobPromises = new Map<string, Promise<Blob>>();
  private readonly objectUrls = new Map<string, string>();
  private readonly objectUrlPromises = new Map<string, Promise<string>>();
  private generation = 0;

  constructor(
    assets: SandboxAddonAsset[] = [],
    private readonly requestBlob: (assetId: string) => Promise<Blob>,
  ) {
    for (const asset of assets) {
      const path = normalizeAddonAssetPath(asset.path);
      if (this.assetsByPath.has(path)) {
        throw new Error(`Duplicate packaged addon asset path '${path}'`);
      }
      this.assetsByPath.set(path, { ...asset, path });
    }
  }

  list(): readonly Omit<SandboxAddonAsset, "id">[] {
    return Array.from(this.assetsByPath.values(), ({ mimeType, path, size }) => ({
      mimeType,
      path,
      size,
    }));
  }

  has(path: string) {
    try {
      return this.assetsByPath.has(normalizeAddonAssetPath(path));
    } catch {
      return false;
    }
  }

  getBlob(path: string): Promise<Blob> {
    const normalizedPath = normalizeAddonAssetPath(path);
    const asset = this.assetsByPath.get(normalizedPath);
    if (!asset) {
      return Promise.reject(new Error(`Packaged addon asset '${normalizedPath}' was not found`));
    }

    const cached = this.blobPromises.get(normalizedPath);
    if (cached) {
      return cached;
    }

    const loading = this.requestBlob(asset.id)
      .then((blob) => {
        if (!(blob instanceof Blob)) {
          throw new Error(
            `Host returned invalid data for packaged addon asset '${normalizedPath}'`,
          );
        }
        if (blob.size !== asset.size) {
          throw new Error(`Packaged addon asset '${normalizedPath}' changed while loading`);
        }
        return blob.type === asset.mimeType ? blob : blob.slice(0, blob.size, asset.mimeType);
      })
      .catch((error: unknown) => {
        this.blobPromises.delete(normalizedPath);
        throw error;
      });
    this.blobPromises.set(normalizedPath, loading);
    return loading;
  }

  getUrl(path: string): Promise<string> {
    const normalizedPath = normalizeAddonAssetPath(path);
    const cached = this.objectUrls.get(normalizedPath);
    if (cached) {
      return Promise.resolve(cached);
    }
    const pending = this.objectUrlPromises.get(normalizedPath);
    if (pending) {
      return pending;
    }

    const generation = this.generation;
    const loading = this.getBlob(normalizedPath)
      .then((blob) => {
        if (generation !== this.generation) {
          throw new Error("Packaged addon asset registry was cleared while loading");
        }
        const url = URL.createObjectURL(blob);
        this.objectUrls.set(normalizedPath, url);
        this.objectUrlPromises.delete(normalizedPath);
        return url;
      })
      .catch((error: unknown) => {
        this.objectUrlPromises.delete(normalizedPath);
        throw error;
      });
    this.objectUrlPromises.set(normalizedPath, loading);
    return loading;
  }

  clear() {
    this.generation += 1;
    for (const url of this.objectUrls.values()) {
      URL.revokeObjectURL(url);
    }
    this.objectUrls.clear();
    this.objectUrlPromises.clear();
    this.blobPromises.clear();
  }
}
