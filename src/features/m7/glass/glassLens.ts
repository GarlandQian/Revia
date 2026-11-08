/**
 * 选中项切换时的玻璃镜片（Q202）。
 *
 * 选中项变化时，一块镜片从旧选中项滑到新选中项：镜片下的内容被放大、在边缘弯折，边缘带红蓝色散；
 * 落定后镜片淡出，露出真实的选中项。只在系统材质下（`html.window-material`）出现。
 *
 * WKWebView 不支持 `backdrop-filter` 引用 SVG 滤镜（docs/milestones/m7/glass-lens-facts.md），
 * 所以镜片里放一份列表的快照（cloneNode），对快照用 `filter: url()` 做位移与色散；WebView2 上同样可用。
 * 动画期间原列表在镜片位置挖空（mask），列表与快照都带 `data-glass-lens`，各控件据此藏起自己的选中底，
 * 由镜片充当选中底。
 */

import { useLayoutEffect, useRef, type RefObject } from "react";
import { supportsGlassLens } from "@/lib/platform";

export type GlassLensOptions = {
  /** 镜片层挂在这里，需要 `position: relative`（`.glass-lens-host`）。 */
  host: RefObject<HTMLElement | null>;
  /** 被拍快照、被挖空的列表。 */
  track: RefObject<HTMLElement | null>;
  /** 当前选中项的键：列表里 `data-<attribute>` 等于它的元素。 */
  selected: string | null;
  /** 选中项键所在的 dataset 名，默认 `lensKey`（`data-lens-key`）。 */
  attribute?: string;
};

const SVG_NS = "http://www.w3.org/2000/svg";
const DURATION = 520;
const FADE = 160;
const EASING = "cubic-bezier(0.45, 0.05, 0.25, 1.15)";
/** 移动到一半时的放大倍数。 */
const MAGNIFY = 1.12;
/** 边缘位移的 scale（最大位移是它的一半，CSS 像素）与红、蓝两道色散影相对原图的差。 */
const BEND = 28;
const DISPERSION = 0.45;

const running = new WeakMap<HTMLElement, () => void>();
let serial = 0;

export function useGlassLens({ host, track, selected, attribute = "lensKey" }: GlassLensOptions): void {
  const previous = useRef(selected);
  useLayoutEffect(() => {
    const from = previous.current;
    previous.current = selected;
    const trackElement = track.current;
    if (!from || !selected || from === selected || !trackElement || !lensEnabled()) return;
    // 连续切换时先收掉上一块镜片，并在这一帧绘制前藏起新的选中底，免得它先闪出来再被镜片盖住。
    running.get(trackElement)?.();
    trackElement.dataset.glassLens = "";
    let started = false;
    // 等同一轮里的滚动定位（例如页签露出）做完再量位置。
    const timer = window.setTimeout(() => {
      const hostElement = host.current;
      started = !!hostElement && play(hostElement, trackElement, attribute, from, selected);
      if (!started) delete trackElement.dataset.glassLens;
    }, 0);
    return () => {
      window.clearTimeout(timer);
      if (!started) delete trackElement.dataset.glassLens;
    };
  }, [host, track, selected, attribute]);
}

function lensEnabled(): boolean {
  // M9 对照外观（S4）与 Windows 降级（Q209）：不播放镜片。
  if (document.documentElement.dataset.plainAppearance === "true") return false;
  if (!supportsGlassLens()) return false;
  if (!document.documentElement.classList.contains("window-material")) return false;
  if (typeof window.matchMedia !== "function") return true;
  return !["(prefers-reduced-motion: reduce)", "(prefers-reduced-transparency: reduce)", "(forced-colors: active)"].some(
    (query) => window.matchMedia(query).matches,
  );
}

type Box = { x: number; y: number; w: number; h: number };

/** 放出镜片；有一头找不到或不在可见范围里时不放，返回 false。 */
function play(host: HTMLElement, track: HTMLElement, attribute: string, fromKey: string, toKey: string): boolean {
  const fromItem = lensItem(track, attribute, fromKey);
  const toItem = lensItem(track, attribute, toKey);
  if (!fromItem || !toItem) return false;

  // 坐标都换算成未缩放的 CSS 像素（界面缩放用的是 CSS zoom），以列表左上角为原点。
  const hostRect = host.getBoundingClientRect();
  const zoom = host.offsetWidth ? hostRect.width / host.offsetWidth : 1;
  const measure = (element: Element): Box => {
    const rect = element.getBoundingClientRect();
    return {
      x: (rect.left - hostRect.left) / zoom,
      y: (rect.top - hostRect.top) / zoom,
      w: rect.width / zoom,
      h: rect.height / zoom,
    };
  };
  const area = measure(track);
  const relative = (box: Box): Box => ({ ...box, x: box.x - area.x, y: box.y - area.y });
  const a = relative(measure(fromItem));
  const b = relative(measure(toItem));
  // 有一头不在可见范围里（滚出列表）就不放镜片。
  if (![a, b].every((box) => box.x + box.w > 0 && box.y + box.h > 0 && box.x < area.w && box.y < area.h)) return false;

  // 镜片的底色与圆角取新选中项本来的样子，落定时与真实选中项无缝衔接：选中底此刻藏着，量之前临时放开（同一帧内，不会画出来）。
  delete track.dataset.glassLens;
  const target = getComputedStyle(toItem);
  const radius = Math.min(Number.parseFloat(target.borderTopLeftRadius) || 8, b.h / 2);
  const fill = target.backgroundColor;

  const id = `revia-glass-lens-${++serial}`;
  const filter = lensFilter(id, b);
  filter.setSize(a.w, a.h);
  track.dataset.glassLens = "";

  const layer = document.createElement("div");
  layer.className = "glass-lens-layer";
  layer.setAttribute("aria-hidden", "true");
  layer.inert = true;
  Object.assign(layer.style, { left: px(area.x), top: px(area.y), width: px(area.w), height: px(area.h) });

  const lens = document.createElement("div");
  lens.className = "glass-lens";
  lens.style.borderRadius = px(radius);
  if (fill && fill !== "rgba(0, 0, 0, 0)" && fill !== "transparent") lens.style.setProperty("--glass-lens-fill", fill);

  const optic = document.createElement("div");
  optic.className = "glass-lens__optic";
  optic.style.filter = `url(#${id})`;

  const holder = document.createElement("div");
  holder.className = "glass-lens__snapshot";
  const snapshot = track.cloneNode(true) as HTMLElement;
  for (const element of [snapshot, ...snapshot.querySelectorAll<HTMLElement>("[id]")]) element.removeAttribute("id");
  Object.assign(snapshot.style, { position: "absolute", left: "0", top: "0", width: px(area.w), height: px(area.h), margin: "0" });
  holder.append(snapshot);
  optic.append(holder);
  lens.append(optic);
  layer.append(lens);
  host.append(filter.svg, layer);
  snapshot.scrollLeft = track.scrollLeft;
  snapshot.scrollTop = track.scrollTop;

  // 镜片与快照都用 left / top 移动：快照若用 transform 动画会被提升成独立的合成层，
  // 光学层上的软件滤镜就作用不到它（WebKit 实测）；两者同在主线程的时间轴上，位置保持同步。
  const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, w: (a.w + b.w) / 2, h: (a.h + b.h) / 2 };
  const timing: KeyframeAnimationOptions = { duration: DURATION, easing: EASING, fill: "forwards" };
  const animations = [
    lens.animate(
      [
        { left: px(a.x), top: px(a.y), width: px(a.w), height: px(a.h), transform: "scale(1)" },
        { left: px(mid.x), top: px(mid.y), width: px(mid.w), height: px(mid.h), transform: "scale(1.04)", offset: 0.5 },
        { left: px(b.x), top: px(b.y), width: px(b.w), height: px(b.h), transform: "scale(1)" },
      ],
      timing,
    ),
    holder.animate([{ left: px(-a.x), top: px(-a.y) }, { left: px(-b.x), top: px(-b.y) }], timing),
    optic.animate([{ transform: "scale(1)" }, { transform: `scale(${MAGNIFY})`, offset: 0.45 }, { transform: "scale(1)" }], timing),
  ];

  // 原列表在镜片位置挖一个同形状的洞，镜片的半透明底下就不会叠出第二份文字。
  Object.assign(track.style, {
    maskImage: `linear-gradient(#000 0 0), url("${pill(b.w, b.h, radius)}")`,
    maskComposite: "exclude",
    maskRepeat: "no-repeat",
  });
  animations.push(
    track.animate(
      [
        { maskPosition: `0 0, ${px(a.x)} ${px(a.y)}`, maskSize: `100% 100%, ${px(a.w)} ${px(a.h)}` },
        { maskPosition: `0 0, ${px(b.x)} ${px(b.y)}`, maskSize: `100% 100%, ${px(b.w)} ${px(b.h)}` },
      ],
      timing,
    ),
  );

  // 位移强度随进度先升后降：起点和终点的快照与原列表完全重合。
  // 用定时器而不是 rAF：窗口被遮挡时 WebKit 暂停 rAF，位移会停在 0（与 nativeGlass.ts 同理）。
  let frame = 0;
  const bend = () => {
    const progress = Math.min(1, Number(animations[0].currentTime ?? 0) / DURATION);
    filter.setSize(optic.offsetWidth, optic.offsetHeight);
    filter.setStrength(Math.sin(Math.PI * progress));
    if (progress < 1) frame = window.setTimeout(bend, 16);
  };
  bend();

  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    window.clearTimeout(frame);
    for (const animation of animations) animation.cancel();
    for (const property of ["mask-image", "mask-composite", "mask-repeat"]) track.style.removeProperty(property);
    delete track.dataset.glassLens;
    layer.remove();
    filter.svg.remove();
    if (running.get(track) === finish) running.delete(track);
  };
  running.set(track, finish);

  void animations[0].finished
    .then(() => {
      if (done) return;
      // 落定：先露出真实的选中项，再让镜片淡出。
      filter.setStrength(0);
      for (const property of ["mask-image", "mask-composite", "mask-repeat"]) track.style.removeProperty(property);
      delete track.dataset.glassLens;
      return layer.animate([{ opacity: 1 }, { opacity: 0 }], { duration: FADE, easing: "ease-out", fill: "forwards" }).finished;
    })
    .catch(() => {})
    .finally(finish);
  return true;
}

function lensItem(track: HTMLElement, attribute: string, key: string): HTMLElement | null {
  for (const element of track.querySelectorAll<HTMLElement>("*")) {
    if (element.dataset[attribute] === key) return element;
  }
  return null;
}

/**
 * 镜片滤镜：位移图让边缘一圈向外取样（把镜片外面的内容折进边缘），中间不动。
 * 色散：快照的底是透明的，按通道拆开再合成不会出彩边（深色字三个通道都是黑的），
 * 所以在原图下面垫红、蓝两道位移量不同的淡影，弯折处就露出红蓝边，深色与浅色字都看得见。
 */
function lensFilter(id: string, size: Box) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "glass-lens-defs");
  svg.setAttribute("aria-hidden", "true");
  const filter = document.createElementNS(SVG_NS, "filter");
  filter.id = id;
  // 区域放大到镜片外，边缘才取得到外面的内容。
  for (const [name, value] of [["x", "-0.5"], ["y", "-1"], ["width", "2"], ["height", "3"]]) filter.setAttribute(name, value);
  filter.setAttribute("color-interpolation-filters", "sRGB");
  const map = document.createElementNS(SVG_NS, "feImage");
  for (const [name, value] of [["x", "0"], ["y", "0"], ["preserveAspectRatio", "none"], ["result", "map"]]) map.setAttribute(name, value);
  map.setAttribute("href", displacementMap(size.w, size.h));
  filter.append(map);
  // 色散影只保留原图的透明度，颜色换成固定的红 / 蓝；color 为 null 的是原图本身。
  const layers = [
    { name: "red", factor: 1 + DISPERSION, color: [1, 0.2, 0.25] },
    { name: "blue", factor: 1 - DISPERSION, color: [0.2, 0.45, 1] },
    { name: "image", factor: 1, color: null },
  ];
  const displacements: [SVGFEDisplacementMapElement, number][] = [];
  const tints: [SVGFEColorMatrixElement, number[]][] = [];
  const merge = document.createElementNS(SVG_NS, "feMerge");
  for (const layer of layers) {
    const displace = document.createElementNS(SVG_NS, "feDisplacementMap");
    for (const [name, value] of [["in", "SourceGraphic"], ["in2", "map"], ["xChannelSelector", "R"], ["yChannelSelector", "G"], ["scale", "0"], ["result", `${layer.name}-shift`]]) {
      displace.setAttribute(name, value);
    }
    filter.append(displace);
    displacements.push([displace, layer.factor]);
    let result = `${layer.name}-shift`;
    if (layer.color) {
      const tint = document.createElementNS(SVG_NS, "feColorMatrix");
      for (const [name, value] of [["in", result], ["type", "matrix"], ["values", tintMatrix(layer.color, 0)], ["result", layer.name]]) tint.setAttribute(name, value);
      filter.append(tint);
      tints.push([tint, layer.color]);
      result = layer.name;
    }
    const node = document.createElementNS(SVG_NS, "feMergeNode");
    node.setAttribute("in", result);
    merge.append(node);
  }
  filter.append(merge);
  const defs = document.createElementNS(SVG_NS, "defs");
  defs.append(filter);
  svg.append(defs);
  return {
    svg,
    /** 位移图铺满镜片当前的尺寸：图外的取值是 0，会把内容整体推开。 */
    setSize(width: number, height: number) {
      map.setAttribute("width", String(width));
      map.setAttribute("height", String(height));
    },
    /** 位移与色散影一起随强度变化：强度为 0 时只剩原图，文字边缘不带色晕。 */
    setStrength(strength: number) {
      for (const [displace, factor] of displacements) displace.setAttribute("scale", (BEND * factor * strength).toFixed(2));
      for (const [tint, color] of tints) tint.setAttribute("values", tintMatrix(color, 0.5 * strength));
    },
  };
}

function tintMatrix([red, green, blue]: number[], alpha: number): string {
  return `0 0 0 0 ${red}  0 0 0 0 ${green}  0 0 0 0 ${blue}  0 0 0 ${alpha.toFixed(3)} 0`;
}

/** R = 横向、G = 纵向，128 为零点；按 2 倍分辨率生成，边缘更平滑。 */
function displacementMap(width: number, height: number): string {
  const ratio = 2;
  const w = Math.max(2, Math.round(width * ratio));
  const h = Math.max(2, Math.round(height * ratio));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const context = canvas.getContext("2d");
  if (!context) return "";
  const image = context.createImageData(w, h);
  const radius = h / 2;
  // 只有靠边的一圈弯折，中间留一块不动的区域，文字保持清楚。
  const band = Math.min(radius * 0.55, 9 * ratio);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      // 到胶囊中轴线段的距离：越靠边，向外取样越多。
      const cx = Math.min(Math.max(x + 0.5, radius), w - radius);
      const dx = x + 0.5 - cx;
      const dy = y + 0.5 - radius;
      const distance = Math.hypot(dx, dy);
      const edge = Math.min(1, Math.max(0, (distance - (radius - band)) / band));
      const strength = edge ** 1.6;
      const nx = distance ? dx / distance : 0;
      const ny = distance ? dy / distance : 0;
      const index = (y * w + x) * 4;
      image.data[index] = 128 + nx * strength * 127;
      image.data[index + 1] = 128 + ny * strength * 127;
      image.data[index + 2] = 128;
      image.data[index + 3] = 255;
    }
  }
  context.putImageData(image, 0, 0);
  return canvas.toDataURL();
}

/** 挖洞用的圆角矩形，按落点尺寸画；移动中的尺寸差别由拉伸近似。 */
function pill(width: number, height: number, radius: number): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none"><rect width="${width}" height="${height}" rx="${radius}"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

function px(value: number): string {
  return `${Math.round(value * 100) / 100}px`;
}
