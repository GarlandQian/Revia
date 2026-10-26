# M3 Q115 原始验证证据

原始命令元数据、stdout/stderr、首次失败、调用时源码diff和逐文件校验、固定Release只读冒烟均完整保存在`raw-evidence.tar.gz`，未清理或改写原始日志。压缩避免原始日志/patch的格式空白被误当成产品代码问题，也确保被`*.log`忽略的原件进入版本库。

`manifest.json`列出精确命令、退出码、归档SHA256和归档内每个文件的SHA256。可用`tar -tzf raw-evidence.tar.gz`列目录；查看或解压不会触发任何测试或应用操作。临时目录原件亦保留。结论与未完成边界见`../../validation-20261026.md`。
