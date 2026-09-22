import {
  COLORS,
  EXPRESSIONS,
  GROKBOT_STATE_NAMES,
  POOLS,
  SHAPES,
  type ColorId,
  type GrokBotAccessoryId,
  type GrokBotPartId,
  type GrokBotStateKey,
  type ShapeId,
} from "./grokbot-data";

export interface GrokBotShareSnapshot {
  color: ColorId;
  shape: ShapeId;
  parts: readonly GrokBotPartId[];
  accessories: readonly GrokBotAccessoryId[];
  state: GrokBotStateKey;
  expression?: number;
}

const DEFAULT_BODY_PATH = SHAPES[0].path;

function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

function ringPath(points: readonly (readonly number[])[]): Path2D {
  const path = new Path2D();
  points.forEach((point, index) => {
    if (index === 0) path.moveTo(point[0], point[1]);
    else path.lineTo(point[0], point[1]);
  });
  path.closePath();
  return path;
}

function drawParts(
  ctx: CanvasRenderingContext2D,
  parts: ReadonlySet<GrokBotPartId>,
  accent: string,
) {
  ctx.save();
  ctx.fillStyle = accent;
  ctx.strokeStyle = accent;
  ctx.lineWidth = 6;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  if (parts.has("antenna")) {
    ctx.beginPath();
    ctx.moveTo(114, 18);
    ctx.lineTo(114, -5);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(114, -12, 8, 0, Math.PI * 2);
    ctx.fill();
  }
  if (parts.has("tail")) {
    ctx.beginPath();
    ctx.moveTo(205, 154);
    ctx.bezierCurveTo(246, 151, 254, 181, 230, 198);
    ctx.bezierCurveTo(216, 208, 214, 220, 227, 228);
    ctx.stroke();
  }
  if (parts.has("hands")) {
    ctx.beginPath();
    ctx.moveTo(25, 132);
    ctx.bezierCurveTo(5, 136, -8, 148, -17, 165);
    ctx.moveTo(204, 132);
    ctx.bezierCurveTo(224, 136, 237, 148, 246, 165);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(-20, 170, 10, 0, Math.PI * 2);
    ctx.arc(249, 170, 10, 0, Math.PI * 2);
    ctx.fill();
  }
  if (parts.has("feet")) {
    ctx.beginPath();
    ctx.moveTo(72, 202);
    ctx.lineTo(72, 224);
    ctx.moveTo(157, 202);
    ctx.lineTo(157, 224);
    ctx.stroke();
    ctx.beginPath();
    ctx.ellipse(62, 230, 24, 10, 0, 0, Math.PI * 2);
    ctx.ellipse(167, 230, 24, 10, 0, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.restore();
}

function drawAccessories(
  ctx: CanvasRenderingContext2D,
  accessories: ReadonlySet<GrokBotAccessoryId>,
  layer: "back" | "front",
) {
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  if (layer === "back" && accessories.has("cape")) {
    ctx.fillStyle = "#7657d8";
    ctx.globalAlpha = 0.88;
    ctx.fill(
      new Path2D("M25 79Q-2 119 13 210Q65 192 90 168ZM204 79Q231 119 216 210Q164 192 139 168Z"),
    );
  }

  if (layer === "front") {
    if (accessories.has("straw-hat")) {
      ctx.fillStyle = "#efcb70";
      ctx.strokeStyle = "#b57b24";
      ctx.lineWidth = 3;
      const crown = new Path2D("M63 28Q72-24 114-28Q156-24 165 28Z");
      ctx.fill(crown);
      ctx.stroke(crown);
      ctx.beginPath();
      ctx.ellipse(114, 31, 91, 18, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = "#d94b5d";
      ctx.fill(new Path2D("M64 10Q114 22 164 10L166 27Q114 38 62 27Z"));
    }
    if (accessories.has("glasses")) {
      ctx.strokeStyle = "#171813";
      ctx.lineWidth = 8;
      ctx.beginPath();
      ctx.arc(72, 108, 37, 0, Math.PI * 2);
      ctx.arc(157, 108, 37, 0, Math.PI * 2);
      ctx.moveTo(109, 106);
      ctx.quadraticCurveTo(114, 99, 120, 106);
      ctx.moveTo(35, 102);
      ctx.lineTo(12, 94);
      ctx.moveTo(194, 102);
      ctx.lineTo(217, 94);
      ctx.stroke();
    }
    if (accessories.has("bowtie")) {
      ctx.fillStyle = "#ff2d8b";
      ctx.fill(
        new Path2D(
          "M114 172L78 151Q62 143 64 176Q65 205 82 194L114 178ZM114 172L150 151Q166 143 164 176Q163 205 146 194L114 178Z",
        ),
      );
      ctx.beginPath();
      ctx.arc(114, 175, 12, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  ctx.restore();
}

/** Render the same 1080×1440 local PNG card as the upstream web edition. */
export function renderGrokBotShareCard(snapshot: GrokBotShareSnapshot): string {
  if (typeof document === "undefined") throw new Error("PNG sharing requires a browser context");

  const canvas = document.createElement("canvas");
  canvas.width = 1080;
  canvas.height = 1440;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas is unavailable in this browser");

  const color = COLORS.find((item) => item.id === snapshot.color) ?? COLORS[6];
  const shape = SHAPES.find((item) => item.id === snapshot.shape);
  const stateName = GROKBOT_STATE_NAMES[snapshot.state];
  const expression = snapshot.expression ?? POOLS[snapshot.state][0] ?? 0;
  const rings = EXPRESSIONS[expression] ?? EXPRESSIONS[0];
  const parts = new Set(snapshot.parts);
  const accessories = new Set(snapshot.accessories);
  const bodyPath = new Path2D(shape?.path ?? DEFAULT_BODY_PATH);

  ctx.fillStyle = "#f1efe8";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = color.hex;
  ctx.font = "700 24px ui-monospace, monospace";
  ctx.fillText("GROKBOT · FACE LAB", 72, 104);
  ctx.fillStyle = "#171813";
  ctx.font = '600 64px Georgia, "Songti SC", serif';
  ctx.fillText("今天的 GrokBot", 72, 184);

  roundedRect(ctx, 72, 250, 936, 840, 48);
  ctx.fillStyle = "#fffdf7";
  ctx.fill();
  ctx.strokeStyle = "#d8d4c8";
  ctx.lineWidth = 3;
  ctx.stroke();

  ctx.save();
  roundedRect(ctx, 72, 250, 936, 840, 48);
  ctx.clip();
  ctx.strokeStyle = "#dedbd2";
  ctx.lineWidth = 2;
  ctx.globalAlpha = 0.72;
  for (let x = 72; x <= 1008; x += 72) {
    ctx.beginPath();
    ctx.moveTo(x, 250);
    ctx.lineTo(x, 1090);
    ctx.stroke();
  }
  for (let y = 250; y <= 1090; y += 72) {
    ctx.beginPath();
    ctx.moveTo(72, y);
    ctx.lineTo(1008, y);
    ctx.stroke();
  }
  ctx.restore();

  ctx.fillStyle = "#74766e";
  ctx.font = "700 22px ui-monospace, monospace";
  ctx.fillText(`GB—${String(expression).padStart(2, "0")}`, 112, 310);
  ctx.fillStyle = "#e36f3d";
  ctx.textAlign = "right";
  ctx.fillText("● LIVE", 968, 310);
  ctx.textAlign = "left";

  ctx.save();
  ctx.shadowColor = "rgba(35,48,80,.24)";
  ctx.shadowBlur = 42;
  ctx.shadowOffsetY = 28;
  ctx.translate(248, 330);
  ctx.scale(2.55, 2.55);
  drawAccessories(ctx, accessories, "back");
  drawParts(ctx, parts, color.hex);
  ctx.fillStyle = color.hex;
  ctx.fill(bodyPath);
  ctx.shadowColor = "transparent";
  ctx.save();
  ctx.clip(bodyPath);
  ctx.fillStyle = "#1a1a2e";
  rings.forEach((ring) => ctx.fill(ringPath(ring)));
  ctx.restore();
  drawAccessories(ctx, accessories, "front");
  ctx.restore();

  const decorationCount = snapshot.parts.length + snapshot.accessories.length;
  roundedRect(ctx, 112, 968, 856, 82, 22);
  ctx.fillStyle = "rgba(255,253,247,.96)";
  ctx.fill();
  ctx.strokeStyle = "#d8d4c8";
  ctx.lineWidth = 2;
  ctx.stroke();
  ctx.fillStyle = "#74766e";
  ctx.font = '400 25px -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif';
  ctx.fillText(
    `${color.name} · ${shape?.name ?? "原始形态"}${decorationCount ? ` · ${decorationCount}件装扮` : ""}`,
    148,
    1019,
  );
  ctx.fillStyle = "#171813";
  ctx.font = '600 27px -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif';
  ctx.textAlign = "right";
  ctx.fillText(`表情 ${String(expression).padStart(2, "0")} · ${stateName}`, 932, 1019);
  ctx.textAlign = "left";

  ctx.fillStyle = "#171813";
  ctx.font = '600 38px Georgia, "Songti SC", serif';
  ctx.fillText(`此刻，我正在${stateName}`, 72, 1198);
  ctx.fillStyle = "#74766e";
  ctx.font = '400 26px -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif';
  ctx.fillText("组合颜色、形状、部件与配饰，遇见独一无二的自己。", 72, 1252);
  ctx.fillStyle = color.hex;
  ctx.font = "700 22px ui-monospace, monospace";
  ctx.fillText("# GROKBOT", 72, 1348);
  ctx.fillStyle = "#96978f";
  ctx.textAlign = "right";
  ctx.font = "400 20px ui-monospace, monospace";
  ctx.fillText("作者：老A玩AI · LOCAL FACE ENGINE", 1008, 1348);
  ctx.textAlign = "left";

  return canvas.toDataURL("image/png");
}
