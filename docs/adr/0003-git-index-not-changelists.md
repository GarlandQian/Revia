# 呈现 Git 暂存区，不做 Changelists

JetBrains 默认用 Changelists 组织提交，Revia 只忠实呈现 Git 原生的暂存区：同一个文件可以同时有已暂存改动和未暂存改动，两者分别显示。同时维护两套改动分组模型，会让“下一次提交到底包含什么”变得含糊，违背“提交可信”优先的取舍；而且 Changelists 只存在于 IDE 内部，终端和其他 Git 工具都看不到。代价是习惯 Changelists 的 JetBrains 用户需要改用暂存区的方式，JetBrains 自己也提供这种模式。
