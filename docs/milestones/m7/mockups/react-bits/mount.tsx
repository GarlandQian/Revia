// Standalone design preview only. No product imports, Git commands or IPC.
import { createRoot, type Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import GlassSurface from './vendor/GlassSurface';
import GlassIcons from './vendor/GlassIcons';
import SpecularButton from './vendor/SpecularButton';

const buttons: { root: Root; attributes: Record<string, string>; content: string }[] = [];
for (const original of document.querySelectorAll<HTMLButtonElement>('.app-window button.icon-button, .commit-button, .review-button')) {
  const mount = document.createElement('span');
  mount.className = 'rb-button-mount';
  const attributes = Object.fromEntries([...original.attributes].filter(a => a.name !== 'class').map(a => [a.name, a.value]));
  attributes.className = original.className;
  const content = original.innerHTML;
  original.replaceWith(mount);
  buttons.push({ root: createRoot(mount), attributes, content });
}

const surfaces: { root: Root; popup: boolean }[] = [];
for (const element of document.querySelectorAll('.app-window, .popover, .commit-popup')) {
  const mount = document.createElement('div');
  mount.className = 'rb-surface-mount';
  mount.setAttribute('aria-hidden', 'true');
  element.prepend(mount);
  surfaces.push({ root: createRoot(mount), popup: !element.classList.contains('app-window') });
}

const rail = document.querySelector('.tool-rail')!;
rail.replaceChildren();
const railTop = document.createElement('div');
const railSpacer = document.createElement('span');
railSpacer.className = 'spacer';
const railBottom = document.createElement('div');
railTop.className = railBottom.className = 'rb-rail-mount';
rail.append(railTop, railSpacer, railBottom);
const topRoot = createRoot(railTop), bottomRoot = createRoot(railBottom);
const glyph = (name: string) => <i className={'codicon codicon-' + name} />;
const previewNotice = () => {
  document.getElementById('preview-note')!.textContent = '图标外观预览 · 未切换真实工具窗口';
};
const items = [
  { id: 'commit', label: '提交', icon: glyph('source-control'), className: 'is-selected', onClick: previewNotice },
  { id: 'log', label: '日志', icon: glyph('history'), onClick: previewNotice },
  { id: 'pr', label: 'GitHub PR', icon: glyph('git-pull-request'), onClick: previewNotice },
];
const terminal = [{ id: 'terminal', label: '终端', icon: glyph('terminal'), onClick: previewNotice }];
const forced = matchMedia('(forced-colors: active)');
const reduced = matchMedia('(prefers-reduced-transparency: reduce)');

function update() {
  const dark = document.documentElement.dataset.theme !== 'light';
  const flat = forced.matches || reduced.matches || document.documentElement.dataset.transparency === 'reduced';
  const mac = document.documentElement.dataset.platform === 'macos';
  flushSync(() => {
    for (const { root, attributes, content } of buttons) {
      const primary = attributes.id === 'commit-submit';
      root.render(
        <SpecularButton {...attributes} size="sm" radius={5} tint={dark ? '#c3d2dd' : '#405b71'}
          tintOpacity={primary ? (dark ? 0.16 : 0.12) : 0.045} textColor={dark ? '#e5ebf0' : '#294155'}
          lineColor={dark ? '#c2d5e3' : '#ffffff'} baseColor={dark ? '#556573' : '#adbcc7'}
          intensity={flat ? 0 : 0.55} idleIntensity={flat ? 0 : 0.18} glow={0.25} thickness={1}
          blur={flat ? 0 : 10} autoAnimate={false} followMouse={!flat} proximity={80}>
          <span className="rb-label" dangerouslySetInnerHTML={{ __html: content }} />
        </SpecularButton>
      );
    }
    for (const { root, popup } of surfaces) {
      root.render(<GlassSurface width="100%" height="100%" borderRadius={popup ? 7 : mac ? 12 : 8}
        borderWidth={0.04} backgroundOpacity={0.1} brightness={55} saturation={1.05}
        distortionScale={-80} redOffset={0} greenOffset={5} blueOffset={10} blur={12}
        refraction={!flat} className={popup ? 'rb-popup-surface' : 'rb-window-surface'} />);
    }
    const props = { size: 28, gap: 13, columns: 1, roundness: 0.36, refraction: flat ? 0 : 0.7,
      bevel: 0.4, dispersion: 0.1, frost: 0.2, shine: 0.45, tint: dark ? '#b7c9d6' : '#ffffff',
      tintOpacity: dark ? 0.08 : 0.15, iconColor: dark ? '#d4e0e8' : '#3c5265',
      plate: 'none' as const, hover: 'press' as const, parallax: 0, labels: 'none' as const,
      intro: false, theme: dark ? 'dark' as const : 'light' as const };
    topRoot.render(<GlassIcons {...props} items={items} />);
    bottomRoot.render(<GlassIcons {...props} items={terminal} />);
  });
  document.documentElement.dataset.reactBits = 'mounted';
}

Object.assign(window, { updateReactBits: update });
forced.addEventListener('change', update);
reduced.addEventListener('change', update);
update();
