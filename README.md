<p align="center">
  <img src="pics/1785392014547.jpg" alt="Laozhou" width="180">
</p>

# 老周（Laozhou）

一个活在终端里的 Linux 运维老油条。

## 项目背景

本项目基于开源项目 [Miyu](https://github.com/SHORiN-KiWATA/Miyu) 的框架进行改造。Miyu 是一个活在终端里的二次元少女 AI 助手，由 [SHORiN-KiWATA](https://github.com/SHORiN-KiWATA) 开发，使用 MIT 协议开源。在保留 Miyu 原有架构和核心功能的基础上，本项目对人设提示词、角色定位、交互风格进行了全面改造，将其从一个二次元少女角色转变为一个务实的 Linux 运维工程师角色。

### "老周"是谁？

"老周"的角色设定来源于本项目创建者 **乌龙** 的同事——一位真实的运维工程师老周。现实中的老周是一位干了近二十年的 Linux 运维老油条，43 岁，江苏人，技术扎实但被社会毒打够了，对生活没指望，每天睡不好，动不动想辞职。他排斥 AI，信自己手搓的东西，问题能用就行，不整花活。

本项目将这位真实人物的性格特征、技术习惯、说话方式提炼为 AI 人设，在终端里复刻了一个"老周"——你问他技术问题，他嘴上嫌烦但会认真帮你排障；你说"装个 wps"，他直接甩你一条 `yay -S wps-cn`。

## 核心功能

`laozhou` 由大模型驱动，默认接入了 [opencode](https://github.com/anomalyco/opencode) 的公共模型服务，你也可以配置自己的大模型服务。他的核心定位是 Linux 桌面助手，偏向系统排障、指令转化、运维管理、日常聊天等场景，而非专业的 Coding Agent。并且 `laozhou` 还可以无缝与 `fish`、`zsh`、`bash` 集成，终端打字直接无缝对话！

![](./pics/shell-init.png)

`laozhou` 还自带了 TUI 方便修改配置。

```
laozhou config
```

![](./pics/tui.png)

## 如何安装？

- Arch Linux

  从 [Releases](https://github.com/yuanjingINT/Laozhou/releases) 下载 `laozhou-*.pkg.tar.zst`，然后：

  ```
  sudo pacman -U laozhou-*.pkg.tar.zst
  ```

  安装后可复制内置的示例人格和用户身份到用户目录（可选）：

  ```
  cp /usr/share/laozhou/personas/*.md ~/.config/laozhou/prompts/
  cp /usr/share/laozhou/identities/*.md ~/.config/laozhou/identities/
  ```

- 从源码构建

  需要安装 Rust 1.96 或更新版本、C 编译工具链、`pkg-config` 和 ALSA 开发库，图片显示功能依赖 `chafa`。Arch Linux、Fedora 和 Ubuntu 24.04 均已验证可构建。

  ```
  git clone https://github.com/yuanjingINT/Laozhou.git
  cd Laozhou
  cargo build --release --locked
  ./target/release/laozhou --version
  ```

  各发行版依赖示例：

  ```
  # Arch Linux
  sudo pacman -S --needed rust cargo pkgconf alsa-lib gcc

  # Fedora
  sudo dnf install cargo rust rust-std-static pkgconf-pkg-config alsa-lib-devel gcc

  # Ubuntu 24.04
  sudo apt install curl build-essential pkg-config libasound2-dev ca-certificates
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
  ```

### 如何卸载？

- Arch Linux（pacman 安装）

  ```
  sudo pacman -Rns laozhou
  ```

- 源码构建

  直接删除二进制和用户数据目录：

  ```
  rm -f ~/.local/bin/laozhou
  rm -rf ~/.config/laozhou ~/.local/share/laozhou
  ```

  >注意：`~/.local/share/laozhou/kb` 下是你的知识库数据，删了就没了，删之前先备份。

### 界面语言

Laozhou 的 CLI、REPL、配置 TUI 和工具状态支持英文与简体中文。在 `laozhou config` 的"全局设置 / Global Settings"中可将"界面语言 / Interface language"设为：

- `auto`：默认值，跟随系统 locale
- `en`：英文
- `zh`：简体中文

`LAOZHOU_LANG=en` 或 `LAOZHOU_LANG=zh` 可以临时覆盖配置。语言选择优先级为 `LAOZHOU_LANG`、`display.language`、系统 locale；在配置 TUI 中保存后，下次启动 Laozhou 时生效。

### 快速开始

1. **初始化配置**

  安装后首次运行，老周会自动生成默认配置：

  ```
  laozhou
  ```

  或手动初始化：

  ```
  laozhou init
  ```

2. **配置模型服务**

  默认使用 [opencode](https://github.com/anomalyco/opencode) 公共模型服务，开箱即用。如需使用自己的 API（OpenAI、DeepSeek、本地 Ollama 等）：

  ```
  laozhou config
  ```

  在 TUI 中配置 Provider 和模型即可。

3. **选择 AI 人格和用户身份**（可选）

  在 TUI 中可以切换老周的人格定位和你的用户身份：

  ```
  laozhou config
  # → AI 人格 → 选择后端开发/DBA/安全工程师/全栈/网络工程师等
  # → 用户身份 → 选择 linux小白/老板/默认
  ```

  人格决定老周的专业领域和说话风格，用户身份决定老周对你的回答方式。两者可独立组合。

4. **集成终端**（可选但强烈推荐）

  让老周住进你的 shell，敲命令时自然语言直接转化：

  ```
  # bash
  laozhou bash-init >> ~/.bashrc

  # zsh
  laozhou zsh-init >> ~/.zshrc

  # fish
  laozhou fish-init > ~/.config/fish/conf.d/laozhou.fish
  ```

  重开终端后，直接在终端里说话就能触发老周。

5. **开始对话**

  ```
  # 单次问答
  laozhou ask "N卡怎么装驱动"

  # 进入 REPL
  laozhou

  # 查看所有命令
  laozhou --help
  ```

### 配置文件位置

| 用途 | 路径 |
| --- | --- |
| 主配置 | `~/.config/laozhou/config.json` |
| AI 人格 | `~/.config/laozhou/prompts/` |
| 用户身份 | `~/.config/laozhou/identities/` |
| 知识库数据 | `~/.local/share/laozhou/kb/` |
| 知识库元数据 | `~/.local/share/laozhou/kb_meta.db` |
| 语义索引 | `~/.local/share/laozhou/semantic_index.db` |
| 表情包 | `~/.local/share/laozhou/memes/` |
| 日志 | `~/.local/share/laozhou/logs/` |
| Shell Hook | `~/.config/laozhou/shell/` |
| 内置示例资源 | `/usr/share/laozhou/`（personas / identities / default-kb / memes） |

### AI 人格与用户身份

Laozhou 支持两个独立的维度来定制对话体验：

- **AI 人格**：决定老周的专业领域和说话风格。默认是"运维老油条"，内置示例人格覆盖后端开发、网络工程师、DBA、安全工程师、全栈开发等职业，每种职业搭配不同的语言风格（温和佛系、老干部、丧系话痨、热血奋斗、冷幽默等）。也保留了原版 Miyu 的"未有"二次元少女家人格，可在 `laozhou config` 中切换。
- **用户身份**：决定老周对你的回答方式。内置 3 种：`linux小白`（命令附说明、分步骤、危险操作警告）、`老板`（只给结论和风险、不甩命令、给方案选项）、`默认`（有一定基础、正常节奏）。

在 `laozhou config` 的 TUI 中可独立选择两者，组合使用。例如"老徐（网络工程师·老干部风）+ 老板身份"会让老徐用体制腔向老板汇报网络问题。

### 人设批量生成器

Laozhou 内置了基于"老周"模板的人设生成器 `persona_generator.py`，可批量生成结构一致但特征各异的人设 prompt：

```
# 生成 10 个随机人设并安装到人格目录
python3 /usr/share/laozhou/persona_generator.py -n 10 --install --no-json

# 指定职业类型生成
python3 /usr/share/laozhou/persona_generator.py --persona-type security -n 3 --install --no-json

# 指定语言风格
python3 /usr/share/laozhou/persona_generator.py --language-style 毒舌刻薄 --install --no-json

# 查看所有可配置维度
python3 /usr/share/laozhou/persona_generator.py --list
```

支持 20 个可配置参数维度（核心身份 / 语言风格 / 专业领域 / 人设特征 / 行为约束），10 种职业类型 × 8 种语言风格 × 16 种性格，MD5 指纹去重保证唯一性。

### 内置插件

<details><summary>[展开/收起] 具体介绍</summary>
<br>

- 表情包

  表情包毫无疑问是聊天时最重要的部分，在对话时，Laozhou 会根据情景自主发送符合情境的表情包。除了自主发送，设置里还可以设置概率、置信度和冷却时间。

  ![](./pics/nvidiafuckyou.png)

  Laozhou 自带了一些表情，存放在`/usr/share/laozhou`，对应的用户空间目录是`~/.local/share/laozhou`。表情库是跟随人格的，如果你在设置里新建了自己的人格，那么就无法使用 Laozhou 的默认表情。你可以准备一些图片，把路径给 Ai，让其保存到表情库。届时会自动调用识图模型对图片进行分析并保存。Laozhou 默认使用 opencode 公共模型服务中的多模态模型进行识图，所以即使不配置自己的多模态模型也可以看图片。

- 玄学算命

  >心理学。

  算命就像看天气预报一般稀松平常。Laozhou 自带了周易六十四卦、吉凶占、塔罗牌抽取等玄学功能。

  ![](./pics/玄学.png)

  ![](./pics/吉凶占.png)

- 投骰子

  >赌！

  闲来无事可以和 AI 比比大小。

  ![](./pics/骰子.png)

- 闹钟

  >要我说，这比GNOME时钟的闹钟好用多了

  Laozhou 自带了闹钟，日常泡泡面、番茄钟学习、计时任务什么的都很实用。内置了闹钟音频，你还可以通过路径传入你想要在到点后播放的"闹钟"。

  ![](./pics/set_alarm.png)

- 知识库

  Laozhou 自带了 [ShorinWiki](https://github.com/SHORiN-KiWATA/Shorin-ArchLinux-Guide) 中的内容和一些日用 Linux 会遇到的问题作为默认知识库。

  当然，你也可以通过 `laozhou kb` 命令，或者通过跟 AI 的自然语言交互管理属于你自己的知识库。

  ![](./pics/kb.png)

  >**搜索结果自动入库**：当老周通过网络搜索找到有价值的技术方案时，会主动询问你是否保存到知识库。保存时会自动整理成结构化的 Markdown（标题、问题描述、解决方案、命令、踩坑点、来源链接），方便后续检索复用。

- ProtonDB 查询

  可以查询 ProtonDB 上的游戏信息和相应的评论，为 Linux 玩游戏提供参考建议。

- Linux 游戏兼容性调查

  >这个游戏 Linux 能玩吗？

  这是桌面端使用 Linux 的日经问题，Laozhou 会去 [ProtonDB](https://www.protondb.com/)、[Are We Anti-Cheat Yet?](https://areweanticheatyet.com/)、[Can I Play On Linux](https://caniplayonlinux.com/)等 Linux游戏兼容性资讯网站获取主要信息，辅以社区玩家的声音，综合判断一款游戏的兼容性并提出建议和注意事项。

  ![](./pics/gaming.png)

- 网络搜索

  即使不配置网络搜索 API，Laozhou 也仍然拥有基础的网络搜索和网页读取能力。可以在插件配置中设置 Tavily、Firecrawl 、AnySearch、SearXNG 等网络搜索 API 以获得更佳的搜索效果。

  ![](./pics/web-search-config.png)

- 搜图

  Laozhou 还能帮你找图片！搜图会根据网络环境并行使用多个来源，并通过视觉模型筛选相关且安全的结果。图片会默认保存至`XDG图片目录/Laozhou`。

  >NSFW 禁止！

  ![](./pics/搜图.png)

- 生图

  支持 OpenAI 的画图服务。图片会默认保存至`XDG图片目录/Laozhou`。

  >这个功能默认用不了，要自己在插件设置里开启并配置 API

  ![](./pics/生图.png)

- 天气查询

  查询天气是每天的必做活动，当然少不了。

  ![](./pics/weather.png)

- 汇率查询

  国际社会，查个汇率也很合理吧？

  ![](./pics/汇率.png)

- Man 手册查询

  >Man！

  专门的手册查询工具，虽然网络搜索也能做到，但这值得做成单独的插件。

  ![](./pics/man.png)

- Arch Linux相关

  Arch Linux 是桌面 Linux 的热门之选，Laozhou 有一系列插件可以帮助提高 Arch Linux 的日用体验。

  - AUR 状态查询

    >AUR 还在被 DDos 吗！

    AUR 的状态是日用 Arch 时的重要信息之一，不访问网站就能查询的话，在 AUR 安装出现异常时查起来会方便很多。

    ![](./pics/aur-status.png)

  - AUR 包查询

    可以查询 AUR 上的包的具体信息

  - Arch Wiki 查询

    作为 "Linux 圣经"，查询 Arch Wiki 不仅能提高日用 Arch 的体验，对其他发行版也大有裨益。

    ![](./pics/archwiki.png)

  - PKGBUILD 审查

    AUR 投毒的事件搞得人心惶惶，但现在，Laozhou 可以帮忙审查 PKGBUILD 啦！

    ![](./pics/pkgbuild审核.png)

- 文件操作

  >自不必说。

  Laozhou 支持读写文件、搜索内容、查找文件、删除文件等。

- 计算器和哈希编解码

  为了计算结果的准确性，Laozhou 自带了科学计算器和哈希编解码的能力。

  ![](./pics/hash.png)

- 记忆系统

  Laozhou 的记忆由两部分组成，其一是"曾经发生的事"，其二是"信息中的知识点"。对话时会根据用户消息自动召回条目，这是联想功能。

  ![](./pics/记忆.png)

- 深度研究

  >Token 燃烧警告

  重量级插件。对于一个命题，Laozhou 可以引经据典，有理有据地进行深度研究并写出研究报告。

  ![](./pics/深度研究.png)

- Linux 输入法问题诊断

  从 Linux 输入法实现原理出发，对软件输入法问题进行深度诊断。

- Fcitx5 wiki 查询

  阅读 Fcitx5 wiki，为输入法问题提供参考。

- 语音对话

  >开口就能使唤老周。

  Laozhou 支持语音对话模式：自定义唤醒词唤醒 → 语音转文字（STT）→ AI 回答 → 文字转语音（TTS）朗读。

  ```
  laozhou voice
  ```

  首次使用前需要在 `laozhou config` → 插件配置 → 语音 中配置：
  - **语音识别后端**：默认 `whisper-cli`（whisper.cpp），需安装并配置模型路径；也可选 `xiaomi`（小米 MiMo ASR，需 API Key）
  - **语音合成后端**：默认 `espeak-ng`，可选 `piper`、`xiaomi`（小米 MiMo TTS）或自定义命令
  - **唤醒词**：自定义，如 `老周`、`laozhou`，可 `laozhou voice --wake-word 老周` 临时覆盖

  使用小米 MiMo 语音时，需在语音插件配置里填写 `xiaomi_api_key`（支持 `$env:变量名` 引用环境变量）。默认接入小米 MiMo 开放平台（`xiaomi_base_url` 默认 `https://api.xiaomimimo.com/v1`），模型为 `mimo-v2.5-asr`（语音识别）与 `mimo-v2.5-tts`（语音合成），可用 `xiaomi_tts_voice` 选择音色（如 `冰糖`、`茉莉`、`苏打`、`Mia` 等）。

  常用参数：`--no-wake` 跳过唤醒词直接监听，`--once` 完成一次问答后退出，`--no-tts` 不朗读回复。录音后端自动检测 `pw-record`/`parec`/`arecord`。

  完整交互流程：喊唤醒词（如「老周」）或**按空格键** → 老周用语音回应「我在」→ 开始录音 → 你说出指令 → 停顿 `silence_ms` 毫秒（默认 5000）后自动结束录音 → 转文字 → AI 回答 → 语音朗读。

  语音对话使用全屏可视化界面：中央是一个水波圆形（监听时涟漪、录音时波纹扩散、思考时水滴转圈、说话时随声音波动），下方滚动显示对话内容。按 `Esc` 或 `q` 退出，按空格手动唤醒。

  >**麦克风底噪大导致唤醒不灵敏？** 在语音插件配置中把「输入设备 / Input device」设为 `denoised_source`，Laozhou 会通过 PipeWire WebRTC 降噪（`module-echo-cancel`）消除 USB 耳机等麦克风的持续底噪，让唤醒词和语音识别更准确。配置会自动持久化，降噪源缺失时 Laozhou 会按需自动创建。

</details>

## 使用示例

```bash
# 让老周帮你装软件
$ 装个wps
→ 正在搜索 AUR... 找到 wps-office-cn
→ 审查 PKGBUILD...
→ 🟢 安全，可以装
yay -S wps-office-cn

# 排障
$ laozhou ask "Wayland下微信没法输入中文"
→ 老周会从知识库召回相关条目，给出 fcitx5 环境变量配置方案

# 查游戏兼容性
$ laozhou ask "赛博朋克2077 Linux能玩吗"
→ 老周会查 ProtonDB、AreWeAntiCheatYet，给出 🟢/🟡/🔴 评级和注意事项

# 深度研究
$ laozhou ask "对比下 Hyprland 和 Niri"
→ 老周会引经据典，写出有理有据的对比报告

# 语音对话（需先配置语音插件）
$ laozhou voice --wake-word 老周
→ 听到"老周"唤醒后开始录音 → 转文字 → 老周回答 → 语音朗读
```

## 常见问题

<details><summary>[展开/收起]</summary>

**Q：装完之后终端没有无缝对话功能？**

新开一个终端窗口。Shell hook 是写到 `~/.bashrc` / `~/.zshrc` / `~/.config/fish/conf.d/laozhou.fish` 的，旧终端不会自动加载。

**Q：默认的 opencode 公共模型服务够用吗？**

日用排障、指令转化够用。如果需要更强的推理能力或者更快的响应，建议在 `laozhou config` 里配置自己的 API。

**Q：知识库支持语义检索吗？**

支持。默认是关键词检索，在 `laozhou config` 中配置 Embedding 模型后即可启用语义检索。

**Q：老周会上传我的数据吗？**

不会。老周只在调用大模型时把当前对话上下文发给配置的 Provider，知识库、记忆、配置都存在本地 `~/.local/share/laozhou`。

**Q：和原版 Miyu 有什么区别？**

- 人设：二次元少女 → 43 岁运维老油条
- 定位：通用桌面助手 → Linux 运维偏向
- 功能：新增 PKGBUILD 审查、AUR 状态查询、搜索结果自动入库等运维向能力
- 框架：基于 Miyu 原有架构，未做破坏性改动

</details>

## 做出贡献

<details><summary>[展开/收起] 如果你想要一同开发 Laozhou 请先阅读下面的内容</summary>
<br>

### 设计理念

Laozhou 的定位是 Linux 桌面助手，不是 Coding Agent。他更注重系统集成度、实用、日常排障等方面。Laozhou 应该开箱即用，并且足够轻量，不开发超重的 3D 桌宠，不使用 GUI 框架，也不设计需要学习成本的 CLI 选项，尽量通过自然语言和无缝无感的触发方式进行所有的操作。

以下是一些可能的方向：

- 提升系统日常排障能力、系统维护能力

  作为桌面助手，尤其是 Linux 桌面端助手，对日常问题的排障能力是重中之重。他应当能够解决日用系统会遇到的问题，如输入法异常、显卡驱动异常、桌面软件崩溃等。

- 知识和信息

  扩充默认的知识库。增加对软件推荐、游戏兼容性调查、时事新闻、学习辅助等非开发场景下会出现的情景的处理能力。增加知识和信息检索的时效性和可靠性也是关键点。

- 提升角色扮演能力，提高对话真实感

  需要更多提升对话时的真实感的功能。老周的人设核心是"丧但靠谱的运维老油条"，功能开发应围绕这一定位展开。

- 提高和系统的无缝集成

  不使用任何命令作为触发器，能够直接使用自然语言开启对话。目前是通过 Command Not Found 内容交给 Laozhou 的方式做到和终端的无缝集成，但是逐行解释命令的特点导致提示词包含多行内容时每一行都会调用一次，如何支持多行无缝对话是一个需要研究的点。

  终端以外的集成也值得研究，例如做成守护进程，拥有持续运行的能力，监听系统事件，在特定事件发生时做出特定反应等。

- 优化功能和修复 BUG

  在不变更设计语义，不影响现有功能效果的前提下优化运行表现，修复 BUG。已知目前流式输出兼容和工具调用兼容有点问题，不是所有模型都正常。

### 如何 PR

PR时必须提供功能的设计理念，作用场景和实际意义。一个 PR 必须仅包含一个功能，若包含多个功能，应当拆分后提交多个 PR。

</details>

## 致谢

- [Miyu](https://github.com/SHORiN-KiWATA/Miyu) 本项目基于 Miyu 的框架改造，感谢 SHORiN-KiWATA 的开源工作。
- [opencode](https://github.com/anomalyco/opencode) 最好的开源 Coding Agent，提供了默认的公共模型服务。

## 创建者

**乌龙**

"老周"的角色原型来自乌龙的同事——一位真实的 Linux 运维工程师。

## 许可

Laozhou 使用 MIT License 发布，见 `LICENSE`。
