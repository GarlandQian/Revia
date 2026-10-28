/**
 * 左右对照与合并页签窄栏里的弧形色带（设计 4.3，照 Rebased）。
 *
 * 一条色带把左栏的一段行区间连到右栏对应的行区间：上下两条边是三次贝塞尔曲线，
 * 两端各自贴住所在一栏的行。一侧长度为 0（纯新增或纯删除）时，那一端画成 2px 的线。
 */

export interface BandEnds {
  /** 左端的上、下边（相对窄栏顶部的像素）。 */
  leftTop: number;
  leftBottom: number;
  rightTop: number;
  rightBottom: number;
}

const MIN_EDGE = 2;

/** 准备画布：按设备像素比设好尺寸并清空，返回绘图上下文与逻辑尺寸。 */
export function prepareCanvas(canvas: HTMLCanvasElement | null, box: HTMLElement | null) {
  if (!canvas || !box) return null;
  const width = box.clientWidth;
  const height = box.clientHeight;
  if (width <= 0 || height <= 0) return null;
  const ratio = window.devicePixelRatio || 1;
  if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
  }
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, width, height);
  return { context, width, height, styles: getComputedStyle(box) };
}

/** 两端都在可视区外（同一侧之外）时不画。 */
export function bandVisible(ends: BandEnds, height: number): boolean {
  return Math.max(ends.leftBottom, ends.rightBottom) >= 0 && Math.min(ends.leftTop, ends.rightTop) <= height;
}

/** 在 `[x0, x1]` 之间画一条色带。 */
export function drawBand(
  context: CanvasRenderingContext2D,
  x0: number,
  x1: number,
  ends: BandEnds,
  fill: string | CanvasGradient,
  edge?: string,
) {
  const leftBottom = Math.max(ends.leftBottom, ends.leftTop + MIN_EDGE);
  const rightBottom = Math.max(ends.rightBottom, ends.rightTop + MIN_EDGE);
  const middle = (x0 + x1) / 2;
  context.beginPath();
  context.moveTo(x0, ends.leftTop);
  context.bezierCurveTo(middle, ends.leftTop, middle, ends.rightTop, x1, ends.rightTop);
  context.lineTo(x1, rightBottom);
  context.bezierCurveTo(middle, rightBottom, middle, leftBottom, x0, leftBottom);
  context.closePath();
  context.fillStyle = fill;
  context.fill();
  if (edge) {
    context.strokeStyle = edge;
    context.lineWidth = 1;
    context.stroke();
  }
}

/** 从左到右渐变（修改：左边是删除色、右边是新增色）。 */
export function horizontalGradient(
  context: CanvasRenderingContext2D,
  x0: number,
  x1: number,
  from: string,
  to: string,
): CanvasGradient {
  const gradient = context.createLinearGradient(x0, 0, x1, 0);
  gradient.addColorStop(0, from);
  gradient.addColorStop(1, to);
  return gradient;
}
