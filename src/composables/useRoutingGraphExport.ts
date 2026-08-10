import { nextTick } from "vue";
import {
  getRectOfNodes,
  getTransformForBounds,
  type VueFlowStore,
} from "@vue-flow/core";
import { toPng } from "html-to-image";
import { LEGEND_ENTRIES } from "../components/routing-graph/portTypes";

export type ExportResultHandler = (
  result: { success: boolean; message?: string },
  successMessage: string,
) => void;

const EXPORT_PADDING = 60;
const MAX_EXPORT_DIMENSION = 4000;
const LEGEND_STRIP_HEIGHT = 48;

function cssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () =>
      reject(new Error("Failed to load captured graph image"));
    img.src = src;
  });
}

/** Draws a standalone legend strip (swatch + label only, no interactive
 * hint text) below the captured graph — the live `.routing-graph-legend`
 * DOM isn't reused here since it carries keyboard-shortcut hint text that
 * would read oddly baked into a static shared image. */
function drawLegendStrip(
  ctx: CanvasRenderingContext2D,
  width: number,
  top: number,
) {
  ctx.fillStyle = cssVar("--surface-1", "#12151c");
  ctx.fillRect(0, top, width, LEGEND_STRIP_HEIGHT);

  const swatchSize = 12;
  const centerY = top + LEGEND_STRIP_HEIGHT / 2;
  let x = 16;
  ctx.font = "13px sans-serif";
  ctx.textBaseline = "middle";

  for (const entry of LEGEND_ENTRIES) {
    ctx.fillStyle = entry.color;
    ctx.beginPath();
    ctx.arc(x + swatchSize / 2, centerY, swatchSize / 2, 0, Math.PI * 2);
    ctx.fill();
    x += swatchSize + 8;

    ctx.fillStyle = cssVar("--text", "#f4f6fb");
    ctx.fillText(entry.label, x, centerY);
    x += ctx.measureText(entry.label).width + 24;
  }
}

async function composeExportCanvas(
  graphDataUrl: string,
  width: number,
  height: number,
): Promise<HTMLCanvasElement> {
  const image = await loadImage(graphDataUrl);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height + LEGEND_STRIP_HEIGHT;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("Canvas rendering isn't supported here");
  }
  ctx.drawImage(image, 0, 0, width, height);
  drawLegendStrip(ctx, width, height);
  return canvas;
}

function downloadCanvas(canvas: HTMLCanvasElement): Promise<void> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new Error("Failed to render the exported image"));
        return;
      }
      const url = URL.createObjectURL(blob);
      const now = new Date();
      const pad = (n: number) => String(n).padStart(2, "0");
      const filename =
        `pipe-deck-routing-${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
        `-${pad(now.getHours())}${pad(now.getMinutes())}.png`;

      const link = document.createElement("a");
      link.href = url;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      resolve();
    }, "image/png");
  });
}

/** Exports the current routing graph as a PNG, framed to fit every node
 * regardless of the user's current pan/zoom (required so a 10+ node graph
 * or a zoomed-in view doesn't export cropped/near-blank), with a small
 * legend strip appended below. Temporarily repositions the vue-flow
 * viewport to the fit-to-bounds transform for the capture and always
 * restores the user's original viewport afterward, even on failure. */
export async function exportRoutingGraphImage(
  vueFlow: VueFlowStore,
  onResult: ExportResultHandler,
): Promise<void> {
  const nodes = vueFlow.getNodes.value;
  if (nodes.length === 0) {
    onResult({ success: false, message: "Nothing to export yet" }, "");
    return;
  }

  const viewportEl = document.querySelector<HTMLElement>(".vue-flow__viewport");
  if (!viewportEl) {
    onResult({ success: false, message: "Routing graph isn't ready yet" }, "");
    return;
  }

  const bounds = getRectOfNodes(nodes);
  const width = Math.min(
    MAX_EXPORT_DIMENSION,
    Math.round(bounds.width + EXPORT_PADDING * 2),
  );
  const height = Math.min(
    MAX_EXPORT_DIMENSION,
    Math.round(bounds.height + EXPORT_PADDING * 2),
  );
  // `getTransformForBounds`'s padding argument is a *ratio* of width/height
  // (default 0.1 == 10%), not a pixel count — passing EXPORT_PADDING (a
  // pixel value) here was being read as "6000% padding", which floored the
  // computed zoom to `minZoom` almost every time and rendered the graph
  // tiny inside a mostly-empty canvas. The margin is already reserved above
  // via width/height, so no additional padding ratio is needed here.
  const transform = getTransformForBounds(bounds, width, height, 0.1, 2, 0);

  const previousViewport = { ...vueFlow.viewport.value };

  try {
    await vueFlow.setViewport(transform);
    await nextTick();

    const graphDataUrl = await toPng(viewportEl, {
      width,
      height,
      pixelRatio: 1,
      backgroundColor: cssVar("--background", "#0b0d12"),
    });

    const canvas = await composeExportCanvas(graphDataUrl, width, height);
    await downloadCanvas(canvas);
    onResult({ success: true }, "Routing graph exported");
  } catch (error) {
    onResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  } finally {
    await vueFlow.setViewport(previousViewport);
  }
}
