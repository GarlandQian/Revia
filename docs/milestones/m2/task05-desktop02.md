# 任务05第二次固定版本桌面观察（未完成验收）

受测source为234f248175ada05d44dbf639655e07a2957a9dac，产品审查截至804d88dbd344b13992e773c34f3403775fe5653c。Release构建退出0，原始构建记录在`/private/tmp/revia-task05-release02-99feetdx`。二进制SHA256为0056277c807da30663181418be79e54cb248e5722abb00616e18b0309fb3d21a。使用隔离HOME/data和生产入口生成的fixture：`/private/tmp/revia-task05-desktop-46oHkH`，清单`/private/tmp/revia-task05-desktop-prep2-vcN7wR/fixture.json`。普通UI、acceptance开启、自动测量关闭；PID17852。启动时checkout产品源码与构建相符；后来Pi独立实施07，不覆盖该固定bundle。

## 实际看到和执行

- 真实回收区仍列5条记录，归属路径、时间、整文件/单hunk来源可见；单hunk详情明确完整文件恢复，保存5117/当前5115字节。
- 恢复最终确认已显示“恢复完整文件”，清空最终确认已显示“清空全部记录”，不再误用丢弃文案。两处只打开确认并取消，未执行恢复覆盖或清空。截图保留CUA会话，不宣称本地PNG存在。
- 二进制详情正确显示两侧均6字节，不提供文本对比。点击“另存保存版本”实际进入原生Save面板；保存到fixture根的新文件`saved-blob-ui02.bin`。UI显示“已另存保存版本：6字节”；逐字节匹配回收记录保存版本，原文件不同且未被覆盖。
- 打开二进制恢复确认后，规划方仅通过终端将本次fixture的`repo/assets/blob.bin`改为12字节后续版本（变更及前后SHA有日志）。生产watcher刷新后旧最终确认消失。详情原预览不会自动重读；返回列表重新查看后，显示保存6/当前12字节、“丢弃之后又有修改：只能只读对比或另存，不能覆盖”，恢复按钮disabled。不把原预览的暂留文字说成自动更新。
- 在真实LaterChanges详情再次原生另存到第二个新路径`saved-blob-later-ui02.bin`，UI成功6字节；两次输出均匹配记录保存字节且未覆盖12字节后续修改。
- 最终Escape/Cmd-Q正常退出，ps确认17852不存在。HEAD、语义暂存区条目/blob、5条回收记录完全不变，5文件中只有预定外部修改的blob发生变化，其他4个文件字节/存在/权限不变。

## 工具过程与未完成项

原生GoToFolder第一次paste未改变输入，改用setValue后成功；一次AX元素索引失效，刷新完整树后继续，未点击错误入口。第一次独立字节核验脚本误找record.json，实际文件名为saved.json；该次退出1不代表产品失败，改用已保存snapshot中的saved.json后核验通过。这些不删除、不改写成全程无工具错误。

没有实际执行恢复覆盖、恢复删除或清空；没有声称已有目标的后端拒绝经过本轮真实GUI验证。文本LaterChanges只读diff、跨worktree迟到结果/门禁、持久重启与身份未知查看、普通滚动和新版D06仍待完成。此时已收到234f248的Windows五项CI失败，停止进一步功能桌面；下一步先交Pi修复并复核/CI，不能把本次局部通过记成05done。最终任务11的30分钟Q98长测也未开始。


[本轮原始证据清单](./evidence/task05-desktop02/manifest.json)；[对应CI完整失败日志](./evidence/task05-ci01/manifest.json)。
