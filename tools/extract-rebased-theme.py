#!/usr/bin/env python3
"""从本机安装的 Rebased 中提取 Islands Light / Islands Dark 的界面与编辑器配色。

用法：python3 tools/extract-rebased-theme.py [Rebased.app 路径] [输出目录]
默认读取 /Applications/Rebased.app，输出到 docs/milestones/m4/reference/。

主题文件来自 IntelliJ 社区版（Apache 2.0），随 Rebased 打包在
lib/intellij.platform.ide.impl.jar 中。脚本沿 parentTheme / parent_scheme 链合并，
把调色板名字解析成颜色值，输出扁平的键值 JSON，供 M4 照抄 Rebased 外观时查阅。
"""

import json
import re
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

APP = Path(sys.argv[1] if len(sys.argv) > 1 else "/Applications/Rebased.app")
OUT = Path(sys.argv[2] if len(sys.argv) > 2 else "docs/milestones/m4/reference")
JAR = APP / "Contents/lib/intellij.platform.ide.impl.jar"

THEMES = {
    "Darcula": "themes/darcula.theme.json",
    "IntelliJ": "themes/intellijlaf.theme.json",
    "ExperimentalLight": "themes/expUI/expUI_light.theme.json",
    "ExperimentalLightWithLightHeader": "themes/expUI/expUI_light_with_light_header.theme.json",
    "ExperimentalDark": "themes/expUI/expUI_dark.theme.json",
    "Islands Light": "themes/islands/ManyIslandsLight.theme.json",
    "Islands Dark": "themes/islands/ManyIslandsDark.theme.json",
}
SCHEME_FILES = [
    "DefaultColorSchemesManager.xml",
    "themes/expUI/expUI_lightScheme.xml",
    "themes/islands/IslandSchemeDark.xml",
]
OS_KEYS = {"os.mac", "os.windows", "os.linux", "os.default"}


def read_json(jar, name):
    return json.loads(re.sub(r"(?m)^\s*//.*$", "", jar.read(name).decode("utf-8")))


def flatten(d, prefix=""):
    out = {}
    for key, value in d.items():
        full = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict) and set(value) <= OS_KEYS:
            out[full] = value.get("os.mac", value.get("os.default"))
        elif isinstance(value, dict):
            out.update(flatten(value, full))
        else:
            out[full] = value
    return out


def resolve_theme(jar, name):
    chain = []
    while name:
        data = read_json(jar, THEMES[name])
        chain.append(data)
        name = data.get("parentTheme")
    colors, ui = {}, {}
    for data in reversed(chain):
        colors.update(data.get("colors", {}))
        ui.update(flatten(data.get("ui", {})))

    def color(value, depth=0):
        if isinstance(value, str) and value in colors and depth < 10:
            return color(colors[value], depth + 1)
        return value

    return {k: color(v) for k, v in sorted(ui.items())}, {k: color(v) for k, v in sorted(colors.items())}


def read_schemes(jar):
    schemes = {}
    for name in SCHEME_FILES:
        root = ET.fromstring(jar.read(name))
        for el in [root] if root.tag == "scheme" else root.iter("scheme"):
            colors = {o.get("name"): o.get("value") for o in el.findall("./colors/option")}
            attrs = {}
            for o in el.findall("./attributes/option"):
                attr = {"baseAttributes": o.get("baseAttributes")} if o.get("baseAttributes") else {}
                attr.update({v.get("name"): v.get("value") for v in o.findall("./value/option")})
                attrs[o.get("name")] = attr
            schemes[el.get("name")] = (el.get("parent_scheme"), colors, attrs)
    return schemes


def resolve_scheme(schemes, name):
    chain = []
    while name:
        chain.append(schemes[name])
        name = schemes[name][0]
    colors, attrs = {}, {}
    for _, c, a in reversed(chain):
        colors.update(c)
        for key, value in a.items():
            attrs[key] = value if "baseAttributes" in value else {**attrs.get(key, {}), **value}
    return dict(sorted(colors.items())), dict(sorted(attrs.items()))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(JAR) as jar:
        schemes = read_schemes(jar)
        for theme, tag in [("Islands Light", "light"), ("Islands Dark", "dark")]:
            ui, palette = resolve_theme(jar, theme)
            scheme = read_json(jar, THEMES[theme]).get("editorScheme")
            colors, attrs = resolve_scheme(schemes, scheme)
            files = {
                f"islands-{tag}-ui.json": ui,
                f"islands-{tag}-palette.json": palette,
                f"islands-{tag}-editor.json": {"scheme": scheme, "colors": colors, "attributes": attrs},
            }
            for file, data in files.items():
                (OUT / file).write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
            print(f"{theme}: {len(ui)} 个界面键，编辑器配色 {scheme}（{len(colors)} 色，{len(attrs)} 个属性）")


if __name__ == "__main__":
    main()
