import json, re, sys
from pathlib import Path
VS = Path("/Applications/Visual Studio Code.app/Contents/Resources/app/extensions/theme-defaults/themes")
ODP = Path.home() / ".vscode/extensions/zhuangtongfa.material-theme-3.20.3/themes/OneDark-Pro.json"
OL = Path(sys.argv[1])
def jsonc(t):
    t = re.sub(r'(?m)^\s*//.*$', '', t)
    t = re.sub(r',(\s*[}\]])', r'\1', t)
    return json.loads(t)
def load(p):
    d = jsonc(p.read_text())
    colors, tokens = {}, []
    if "include" in d:
        c, tk = load(p.parent / d["include"]); colors.update(c); tokens += tk
    colors.update({k.lower(): v for k, v in d.get("colors", {}).items()})
    tokens += d.get("tokenColors", []) if isinstance(d.get("tokenColors"), list) else []
    return colors, tokens
FALLBACK = {
 "dark": {"foreground":"#cccccc","descriptionforeground":"#ccccccb3","icon.foreground":"#c5c5c5","focusborder":"#007fd4",
  "titlebar.activebackground":"#3c3c3c","titlebar.activeforeground":"#cccccc","titlebar.border":"#00000000",
  "activitybar.background":"#333333","activitybar.foreground":"#ffffff","activitybar.inactiveforeground":"#ffffff66","activitybar.activeborder":"#ffffff","activitybar.border":"#00000000",
  "sidebar.background":"#252526","sidebar.foreground":"#cccccc","sidebar.border":"#00000000","sidebarsectionheader.background":"#00000000","sidebarsectionheader.foreground":"#cccccc","sidebarsectionheader.border":"#cccccc33","sidebartitle.foreground":"#bbbbbb",
  "list.activeselectionbackground":"#04395e","list.activeselectionforeground":"#ffffff","list.inactiveselectionbackground":"#37373d","list.hoverbackground":"#2a2d2e","list.focusoutline":"#007fd4",
  "editor.background":"#1e1e1e","editor.foreground":"#d4d4d4","editorlinenumber.foreground":"#858585","editorlinenumber.activeforeground":"#c6c6c6",
  "editorgroupheader.tabsbackground":"#252526","editorgroupheader.tabsborder":"#00000000","tab.activebackground":"#1e1e1e","tab.activeforeground":"#ffffff","tab.inactivebackground":"#2d2d2d","tab.inactiveforeground":"#ffffff80","tab.border":"#252526","tab.activebordertop":"#00000000","tab.activeborder":"#00000000",
  "statusbar.background":"#007acc","statusbar.foreground":"#ffffff","statusbar.border":"#00000000",
  "button.background":"#0e639c","button.foreground":"#ffffff","button.secondarybackground":"#3a3d41","button.secondaryforeground":"#ffffff",
  "input.background":"#3c3c3c","input.foreground":"#cccccc","input.border":"#00000000","input.placeholderforeground":"#a6a6a6","checkbox.background":"#3c3c3c","checkbox.border":"#6b6b6b",
  "badge.background":"#4d4d4d","badge.foreground":"#ffffff","panel.border":"#80808059","toolbar.hoverbackground":"#5a5d5e50",
  "diffeditor.insertedtextbackground":"#9ccc2c33","diffeditor.removedtextbackground":"#ff000033","diffeditor.insertedlinebackground":"#9bb95533","diffeditor.removedlinebackground":"#ff000033","diffeditor.diagonalfill":"#cccccc33","diffeditorgutter.insertedlinebackground":"#9bb95533","diffeditorgutter.removedlinebackground":"#ff000033",
  "gitdecoration.modifiedresourceforeground":"#e2c08d","gitdecoration.addedresourceforeground":"#81b88b","gitdecoration.deletedresourceforeground":"#c74e39","gitdecoration.untrackedresourceforeground":"#73c991","gitdecoration.conflictingresourceforeground":"#e4676b","gitdecoration.renamedresourceforeground":"#73c991","gitdecoration.stagemodifiedresourceforeground":"#e2c08d",
  "editorwidget.background":"#252526","widget.border":"#00000000","notifications.background":"#252526","notifications.border":"#00000000","textlink.foreground":"#3794ff","editorinfo.foreground":"#3794ff","editorwarning.foreground":"#cca700",
  "breadcrumb.foreground":"#cccccccc","scrollbarslider.background":"#79797966","editorgutter.background":"#1e1e1e","editor.linehighlightbackground":"#00000000"},
 "light": {"foreground":"#616161","descriptionforeground":"#717171","icon.foreground":"#424242","focusborder":"#0090f1",
  "titlebar.activebackground":"#dddddd","titlebar.activeforeground":"#333333","titlebar.border":"#00000000",
  "activitybar.background":"#2c2c2c","activitybar.foreground":"#ffffff","activitybar.inactiveforeground":"#ffffff66","activitybar.activeborder":"#ffffff","activitybar.border":"#00000000",
  "sidebar.background":"#f3f3f3","sidebar.foreground":"#616161","sidebar.border":"#00000000","sidebarsectionheader.background":"#00000000","sidebarsectionheader.foreground":"#616161","sidebarsectionheader.border":"#61616133","sidebartitle.foreground":"#6f6f6f",
  "list.activeselectionbackground":"#0060c0","list.activeselectionforeground":"#ffffff","list.inactiveselectionbackground":"#e4e6f1","list.hoverbackground":"#f0f0f0","list.focusoutline":"#0090f1",
  "editor.background":"#ffffff","editor.foreground":"#000000","editorlinenumber.foreground":"#237893","editorlinenumber.activeforeground":"#0b216f",
  "editorgroupheader.tabsbackground":"#f3f3f3","editorgroupheader.tabsborder":"#00000000","tab.activebackground":"#ffffff","tab.activeforeground":"#333333","tab.inactivebackground":"#ececec","tab.inactiveforeground":"#33333380","tab.border":"#f3f3f3","tab.activebordertop":"#00000000","tab.activeborder":"#00000000",
  "statusbar.background":"#007acc","statusbar.foreground":"#ffffff","statusbar.border":"#00000000",
  "button.background":"#007acc","button.foreground":"#ffffff","button.secondarybackground":"#5f6a79","button.secondaryforeground":"#ffffff",
  "input.background":"#ffffff","input.foreground":"#616161","input.border":"#cecece","input.placeholderforeground":"#767676","checkbox.background":"#ffffff","checkbox.border":"#919191",
  "badge.background":"#c4c4c4","badge.foreground":"#333333","panel.border":"#80808059","toolbar.hoverbackground":"#b8b8b850",
  "diffeditor.insertedtextbackground":"#9ccc2c40","diffeditor.removedtextbackground":"#ff000033","diffeditor.insertedlinebackground":"#9bb95533","diffeditor.removedlinebackground":"#ff000033","diffeditor.diagonalfill":"#22222233","diffeditorgutter.insertedlinebackground":"#9bb95533","diffeditorgutter.removedlinebackground":"#ff000033",
  "gitdecoration.modifiedresourceforeground":"#895503","gitdecoration.addedresourceforeground":"#587c0c","gitdecoration.deletedresourceforeground":"#ad0707","gitdecoration.untrackedresourceforeground":"#007100","gitdecoration.conflictingresourceforeground":"#ad0707","gitdecoration.renamedresourceforeground":"#007100","gitdecoration.stagemodifiedresourceforeground":"#895503",
  "editorwidget.background":"#f3f3f3","widget.border":"#00000000","notifications.background":"#f3f3f3","notifications.border":"#00000000","textlink.foreground":"#006ab1","editorinfo.foreground":"#1a85ff","editorwarning.foreground":"#bf8803",
  "breadcrumb.foreground":"#616161cc","scrollbarslider.background":"#64646466","editorgutter.background":"#ffffff","editor.linehighlightbackground":"#00000000"}}
SCOPES = {"comment":"comment","keyword":"keyword.control","storage":"storage.type","modifier":"storage.modifier","string":"string.quoted","number":"constant.numeric","constant":"constant.language","func":"entity.name.function","type":"entity.name.type","support":"support.type","variable":"variable.other.readwrite","param":"variable.parameter","property":"variable.other.property","operator":"keyword.operator","punct":"punctuation.separator","tag":"entity.name.tag","attr":"entity.other.attribute-name"}
def token(tokens, target):
    best, blen = None, -1
    for r in tokens:
        sc = r.get("scope"); st = r.get("settings", {})
        if not sc or "foreground" not in st and "fontStyle" not in st: continue
        for sel in ([sc] if isinstance(sc, str) else sc):
            for s in sel.split(","):
                s = s.strip()
                if not s or " " in s: continue
                if (target == s or target.startswith(s + ".")) and len(s) >= blen:
                    best, blen = st, len(s)
    return best or {}
themes = [("light-2026", VS/"2026-light.json", "light"), ("one-light", OL, "light"), ("dark-2026", VS/"2026-dark.json", "dark"), ("one-dark-pro", ODP, "dark")]
out = []
for tid, path, kind in themes:
    colors, tokens = load(path)
    c = {**FALLBACK[kind], **colors}
    if "tab.activebackground" not in colors: c["tab.activebackground"] = c["editor.background"]
    if "editorgutter.background" not in colors: c["editorgutter.background"] = c["editor.background"]
    lines = [f'[data-theme="{tid}"] {{', f"  color-scheme: {kind};"]
    for k in sorted(FALLBACK[kind]):
        lines.append(f"  --{k.replace('.', '-')}: {c[k]};")
    lines.append(f"  --syn-default: {c['editor.foreground']};")
    for name, scope in SCOPES.items():
        st = token(tokens, scope)
        lines.append(f"  --syn-{name}: {st.get('foreground', c['editor.foreground'])};")
        if "italic" in (st.get("fontStyle") or ""): lines.append(f"  --syn-{name}-style: italic;")
    lines.append("}")
    out.append("\n".join(lines))
    print(tid, "own colors", len(colors), "tokens", len(tokens))
Path("themes.css").write_text("\n\n".join(out) + "\n")
