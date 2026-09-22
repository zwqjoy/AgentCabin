import { BUILTIN_PET_OPTIONS, DEFAULT_PET_ID } from "./pet-settings";
import type { PetManifest, PetMode } from "./types";
import { getTransport } from "$lib/transport";

export const GRID_COLS = 8;
export const GRID_ROWS = 9;
export const CELL_W = 192;
export const CELL_H = 208;

export const MODE_ROW: Record<PetMode, number> = {
  idle: 0,
  running: 7,
  failed: 5,
  waiting: 6,
  review: 8,
};

export const MODE_FRAMES: Record<PetMode, number> = {
  idle: 6,
  running: 6,
  failed: 8,
  waiting: 6,
  review: 6,
};

export interface SpriteSheet {
  image: CanvasImageSource;
  url: string;
  cols: number;
  rows: number;
  cellW: number;
  cellH: number;
  singleFrame?: boolean;
  custom?: boolean;
  animations?: Partial<Record<PetMode, CanvasImageSource>>;
}

export interface SpriteDrawRect {
  width: number;
  height: number;
  x: number;
  y: number;
}

/** Fit one sprite cell inside any preview or desktop canvas without cropping. */
export function fitSpriteToCanvas(
  canvasWidth: number,
  canvasHeight: number,
  sourceWidth: number,
  sourceHeight: number,
): SpriteDrawRect {
  const scale = Math.min(canvasWidth / sourceWidth, canvasHeight / sourceHeight);
  const width = sourceWidth * scale;
  const height = sourceHeight * scale;
  return {
    width,
    height,
    x: (canvasWidth - width) / 2,
    y: (canvasHeight - height) / 2,
  };
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    if (!url) {
      reject(new Error("Empty image URL"));
      return;
    }
    const img = new Image();
    if (/^https?:\/\//i.test(url)) {
      img.crossOrigin = "anonymous";
    }
    img.onload = () => resolve(img);
    img.onerror = (error) => reject(error);
    img.src = url;
  });
}

export async function loadSpriteSheet(manifest: PetManifest): Promise<SpriteSheet> {
  if (
    manifest.id === "grokbot" ||
    (!manifest.spritesheetUrl && !manifest.spritesheetPath && !manifest.animationUrls)
  ) {
    return {
      image: new Image(),
      url: "",
      cols: 1,
      rows: 1,
      cellW: 128,
      cellH: 128,
      singleFrame: true,
    };
  }

  if (manifest.animationUrls) {
    const entries = await Promise.all(
      Object.entries(manifest.animationUrls).map(async ([mode, url]) => {
        const image = await loadImage(url);
        return [mode as PetMode, image] as const;
      }),
    );
    const animations = Object.fromEntries(entries) as Partial<Record<PetMode, CanvasImageSource>>;
    const idle = animations.idle;
    if (!idle) throw new Error("Custom pet ZIP is missing idle animation");
    const idleImage = idle as HTMLImageElement;
    if (manifest.singleFrame) {
      return {
        image: idle,
        url: manifest.animationUrls.idle,
        cols: 1,
        rows: 1,
        cellW: idleImage.naturalWidth || idleImage.width || 1,
        cellH: idleImage.naturalHeight || idleImage.height || 1,
        singleFrame: true,
        custom: true,
      };
    }
    return {
      image: idle,
      url: manifest.animationUrls.idle,
      cols: 1,
      rows: 1,
      cellW: idleImage.naturalWidth || idleImage.width || 1,
      cellH: idleImage.naturalHeight || idleImage.height || 1,
      custom: true,
      animations,
    };
  }

  const url = manifest.spritesheetUrl || `/pets/${manifest.id}/${manifest.spritesheetPath ?? ""}`;

  return new Promise((resolve, reject) => {
    if (!url) {
      resolve({
        image: new Image(),
        url: "",
        cols: 1,
        rows: 1,
        cellW: 128,
        cellH: 128,
        singleFrame: true,
      });
      return;
    }
    const img = new Image();
    if (/^https?:\/\//i.test(url)) {
      img.crossOrigin = "anonymous";
    }
    img.onload = () => {
      resolve({
        image: img,
        url,
        cols: manifest.gridCols || GRID_COLS,
        rows: manifest.gridRows || GRID_ROWS,
        cellW: manifest.cellWidth || CELL_W,
        cellH: manifest.cellHeight || CELL_H,
        singleFrame: manifest.singleFrame,
        custom: manifest.custom,
      });
    };
    img.onerror = (err) => {
      reject(new Error(`Failed to load spritesheet from ${url}: ${String(err)}`));
    };
    img.src = url;
  });
}

export async function loadCustomManifest(id: string): Promise<PetManifest> {
  return getTransport().invoke<PetManifest>("get_custom_pet", { id });
}

export async function loadPetManifest(id = DEFAULT_PET_ID): Promise<PetManifest> {
  if (id.startsWith("custom:")) return loadCustomManifest(id);
  return loadBuiltinManifest(id);
}

export async function loadBuiltinManifest(id = DEFAULT_PET_ID): Promise<PetManifest> {
  const option =
    BUILTIN_PET_OPTIONS.find((candidate) => candidate.id === id) ??
    BUILTIN_PET_OPTIONS.find((candidate) => candidate.id === DEFAULT_PET_ID)!;
  const singleFrame = option.singleFrame === true;
  const pathParts = option.assetPath.split("/");

  return {
    id: option.id,
    displayName: option.displayName,
    description: option.description,
    spritesheetPath: pathParts.at(-1) ?? option.assetPath,
    spritesheetUrl: option.assetPath,
    gridCols: singleFrame ? 1 : GRID_COLS,
    gridRows: singleFrame ? 1 : GRID_ROWS,
    cellWidth: singleFrame ? 128 : CELL_W,
    cellHeight: singleFrame ? 128 : CELL_H,
    singleFrame,
  };
}
