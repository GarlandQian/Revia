# M4 参照：Rebased 1.1.22 的外观数据

M4 照本机安装的 Rebased 1.1.22 一比一重做界面（Q117、Q119）。本文整理从 Rebased 中提取的主题数据，完整键值在 [reference/](./reference/)（来源与许可见 [NOTICE](./reference/NOTICE.md)）。主题文件只给出颜色和部分尺寸，屏幕上的实际效果仍以 Rebased 窗口截图为准，见末节。

## 主题链

| 外观 | 界面主题 | 继承链 | 编辑器配色 |
| --- | --- | --- | --- |
| 浅色 | Islands Light | Islands Light → Light with Light Header → Light → IntelliJ → Darcula | Light（基于 Default） |
| 深色 | Islands Dark | Islands Dark → Dark → Darcula | Islands Dark（基于 Darcula） |

## 窗口与面板

| 部位 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 窗口底色、主工具栏、图标条、状态栏 | `MainWindow.background`、`MainToolbar.background`、`ToolWindow.Stripe.background`、`StatusBar.background` | `#E9EAEE` | `#26282C` |
| 岛（工具窗口、编辑区） | `ToolWindow.background`、`EditorTabs.background` | `#FFFFFF` | `#191A1C` |
| 普通面板 | `*.background` | `#F7F8F9` | `#191A1C` |
| 岛圆角 | `Island.arc`（紧凑 `.compact`） | 20（16） | 20（16） |
| 岛之间的间距 | `Island.borderWidth`（紧凑） | 6（4） | 6（4） |
| 非活动岛不透明度 | `Island.inactiveAlpha` | 0.56 | 0.56 |
| 分隔线 | `*.separatorColor` | `#E9EAEE` | `#33353B` |
| 图标条选中按钮 | `ToolWindow.Button.selectedBackground` / 前景 | `#3871E1` / `#FFFFFF` | `#3871E1` / `#FFFFFF` |
| 弹层 | `Popup.background` / `Popup.borderColor` | `#FFFFFF` / `#D1D3D9` | `#26282C` / `#33353B` |

## 文字与字体

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 正文 | `*.foreground` | `#000000` | `#D1D3D9` |
| 次要文字 | `*.infoForeground` | `#73767C` | `#73767C` |
| 禁用 | `*.disabledForeground` | `#9FA2A8` | `#4C4F56` |
| 链接 | `Link.activeForeground` | `#2F5EB9` | `#71A1FE` |

Rebased 随应用打包 Inter（Regular、SemiBold、Italic）和 JetBrains Mono，与 Revia 现在打包的字体相同。默认界面字号、macOS 上是否使用 Inter，需要实测。

## 列表与树

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 行高 | `Tree.rowHeight`（紧凑） | 24（22） | 24（22） |
| 缩进 | `Tree.leftChildIndent` / `Tree.rightChildIndent` | 7 / 11 | 7 / 11 |
| 选中（有焦点） | `*.selectionBackground` | `#D0DFFE` | `#2A4371` |
| 选中（无焦点） | `*.selectionInactiveBackground` | `#E9EAEE` | `#33353B` |
| 悬停 | `Tree.hoverBackground` | `#00000008` | `#FFFFFF10` |

## 按钮与控件

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 按钮圆角 | `Button.arc` | 8 | 8 |
| 输入框等控件圆角 | `Component.arc`（紧凑） | 8（6） | 8（6） |
| 默认按钮 | `Button.default.startBackground` / 前景 | `#3871E1` / `#FFFFFF` | `#3871E1` / `#FFFFFF` |
| 控件边框 | `Component.borderColor` | `#D1D3D9` | `#40434A` |
| 焦点 | `*.focusColor` | `#3871E1` | `#3871E1` |
| 图标按钮悬停 / 按下 | `ActionButton.hoverBackground` / `pressedBackground` | `#00000012` / `#00000020` | `#FFFFFF17` / `#FFFFFF29` |
| 主工具栏下拉 | `MainToolbar.Dropdown.hoverBackground`，最大宽度 `maxWidth` | `#00000012`，350 | `#4C5052`，350 |
| 计数徽标 | `Counter.background` / 前景 | `#8B8E94` / `#FFFFFF` | `#73767C` / `#000000` |

## 编辑器页签

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 选中页签底色 | `EditorTabs.underlinedTabBackground` | `#E3EBFE` | `#233558` |
| 下划线高度、圆角 | `EditorTabs.underlineHeight`（紧凑）、`underlineArc` | 4（3）、4 | 4（3）、4 |
| 悬停 | `EditorTabs.hoverBackground` | `#00000012` | `#FFFFFF17` |
| 内边距 | `EditorTabs.tabInsets` | `-6,8,-6,8` | `-6,8,-6,8` |

下划线颜色在主题链里有多个候选值（`*.underlineColor` 为 `#3871E1`），以截图为准。

## 提示条与通知

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 信息提示条 | `Banner.infoBackground` / `infoBorderColor` | `#F7F8FF` / `#BDD3FF` | `#233558` / `#2E4D89` |
| 警告提示条 | `Banner.warningBackground` / `warningBorderColor` | `#FFF6E9` / `#F4CD9A` | `#44321D` / `#694820` |
| 错误提示条 | `Banner.errorBackground` / `errorBorderColor` | `#FFF6F5` / `#FFC4C5` | `#56272B` / `#80383E` |
| 成功提示条 | `Banner.successBackground` / `successBorderColor` | `#F5FAF3` / `#BBDBC2` | `#203B2A` / `#29583C` |
| 提示条圆角 | `Banner.arc`（紧凑） | 12（8） | 12（8） |
| 通知气泡 | `Notification.background` / `borderColor` | `#FFFFFF` / `#D1D3D9` | `#33353B` / `#33353B` |

## 编辑器与 diff

| 用途 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 正文与底色 | `TEXT` | `#080808` on `#FFFFFF` | `#BCBEC4` on `#191A1C` |
| 行号 / 当前行行号 | `LINE_NUMBERS_COLOR` / `LINE_NUMBER_ON_CARET_ROW_COLOR` | `#AEB3C2` / `#767A8A` | `#4B5059` / `#A1A3AB` |
| 当前行 | `CARET_ROW_COLOR` | `#F5F8FE` | `#1F2024` |
| 选区 | `SELECTION_BACKGROUND` | `#A6D2FF` | `#214283` |
| 新增块 | `DIFF_INSERTED` | `#BEE6BE` | `#294436` |
| 删除块 | `DIFF_DELETED` | `#D6D6D6` | `#484A4A` |
| 修改块 | `DIFF_MODIFIED` | `#C2D8F2` | `#385570` |
| 冲突块 | `DIFF_CONFLICT` | `#FFD5CC` | `#45302B` |
| 折叠的未改动区 | `DIFF_SEPARATORS_BACKGROUND` | `#E4E6EB` | `#2B2D30` |
| 行侧改动标记（新增 / 修改 / 删除） | `ADDED_LINES_COLOR` / `MODIFIED_LINES_COLOR` / `DELETED_LINES_COLOR` | `#7FC784` / `#88ADF7` / `#767A8A` | `#549159` / `#375FAD` / `#868A91` |

IntelliJ 在开启单词级高亮时，整块背景会画得比上表更浅，只有改动的单词用整色；屏幕上的实际颜色以截图为准。

## 文件状态颜色（Q128）

| 状态 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 修改、重命名 | `FILESTATUS_MODIFIED`、`FILESTATUS_RENAMED` | `#0033B3` | `#70AEFF` |
| 新增 | `FILESTATUS_ADDED` | `#067D17` | `#73BD79` |
| 删除 | `FILESTATUS_DELETED` | `#6C707E` | `#6F737A` |
| 未跟踪 | `FILESTATUS_UNKNOWN` | `#B23247` | `#E88F89` |
| 冲突 | `FILESTATUS_IDEA_FILESTATUS_MERGED_WITH_CONFLICTS` | `#DE1B2E` | `#DE6A66` |

## 语法高亮

| 类别 | 键 | 浅色 | 深色 |
| --- | --- | --- | --- |
| 关键字 | `DEFAULT_KEYWORD` | `#0033B3` | `#CF8E6D` |
| 字符串 | `DEFAULT_STRING` | `#067D17` | `#6AAB73` |
| 数字 | `DEFAULT_NUMBER` | `#1750EB` | `#2AACB8` |
| 注释（斜体） | `DEFAULT_LINE_COMMENT`、`DEFAULT_BLOCK_COMMENT` | `#8C8C8C` | `#7A7E85` |
| 文档注释（斜体） | `DEFAULT_DOC_COMMENT` | `#8C8C8C` | `#5F826B` |
| 函数声明 | `DEFAULT_FUNCTION_DECLARATION` | `#00627A` | `#56A8F5` |
| 常量、字段 | `DEFAULT_CONSTANT`、`DEFAULT_INSTANCE_FIELD` | `#871094` | `#C77DBB` |
| 注解、元数据 | `DEFAULT_METADATA` | `#9E880D` | `#B3AE60` |
| 普通标识符 | `DEFAULT_IDENTIFIER` | `#000000` | `#BCBEC4` |

## 还要在 Rebased 窗口里实测的

需要在 Mac 前打开 Rebased 1.1.22，用同一个测试仓库逐屏截图（浅色、深色各一套），量出主题文件里没有的数值：

- 尺寸：默认界面字号与字体、主工具栏高度、工具窗口标题高度、编辑器页签高度、状态栏高度、图标尺寸。
- diff：块背景和单词级高亮在屏幕上的实际颜色。
- 屏幕：欢迎界面（没有打开仓库时）、主窗口、提交工具窗口（暂存区模式）、diff 页签（并排、统一）、仓库与 worktree 下拉、提交选项弹层、丢弃确认对话框、通知气泡、设置对话框。
- 交互：单击和双击文件分别做什么、多选与“全部暂存”的入口、暂存区模式下 diff 里的暂存按钮、提交后的结果与 hook 输出在哪里显示、通知的位置和停留时间。
- 更改列表模式（Q120）：终端里已经暂存的内容在勾选提交时怎样处理，记录给 M7。
