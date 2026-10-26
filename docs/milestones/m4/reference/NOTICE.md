# 来源与许可

本目录的 JSON 由 `tools/extract-rebased-theme.py` 从本机安装的 Rebased 1.1.22（build 262.10968）中提取：Islands Light、Islands Dark 两套界面主题，以及它们使用的编辑器配色（Light、Islands Dark）。原始文件是 IntelliJ 社区版的主题与配色方案，随 Rebased 打包在 `lib/intellij.platform.ide.impl.jar` 中。

IntelliJ 社区版以 Apache License 2.0 发布，版权归 JetBrains s.r.o. 所有：<https://github.com/JetBrains/intellij-community>。本目录只做了合并与颜色名解析，没有改动取值。

重新生成：

```bash
python3 tools/extract-rebased-theme.py /Applications/Rebased.app docs/milestones/m4/reference
```
