# ModelBox-RS｜新窗口完整上下文交接（2026-10-11）

> **第一步请读** [TODO.md](TODO.md)，它是可执行、带验收条件的待办清单。本文件负责描述工程动机、已完成的源码能力、准确的性能证据、历史判断、外部依赖与协作方式。  
> 本交接以 GitHub 仓库 <https://github.com/hu-qi/modelbox-rs> 的 **main `22f0e129270b4c7fcd2ae402fc292678ada25582`** 为冻结点；后续新窗口应先 `git pull origin main` 并重新检查新提交/PR，避免把冻结点误当当前 HEAD。

## 0. 可直接复制给新窗口的续接指令

> 接管 GitHub 仓库 `hu-qi/modelbox-rs`，先阅读 `docs/NEXT-SESSION-CONTEXT.md` 与 `docs/TODO.md`，核验最新 main、最近 PR 及 5 个 CI Job。当前目标是 CPU-first Rust ModelBox-like Graph/DAG AI Runtime：MNIST 10,000 张真实标注集准确率 98.90%，YOLOv8n 实景公交照片检测五框完成真实端到端验收，Mac mini Release 平均 28.9208 ms；Graph 同 Session ONNX 测量比 Direct 慢 1.5037 ms（8.04%），21 组均为正，但不是纯调度成本。PR #23 已加入 `bench-transport-yolov8` 无模型同尺寸传输控制，PR #24 增加真实 Buffer 指针不变的校验；此控制的 **Mac mini Release 实测数据尚未提供**。请从 TODO **P0-01** 开始，待拿到实测报告再归档、分析和决定是否改动 Worker/Channel，保持 PR/CI 全绿后合并；不要虚构性能、独立准确率或实时视频 FPS。

## 1. 项目意图、边界与技术栈

- **项目**：独立的 ModelBox-like Rust 数据流 AI Runtime 原型，仓库 `hu-qi/modelbox-rs`；参考项目 <https://github.com/modelbox-ai/modelbox>。与上游 C++ ModelBox **没有**代码/插件 ABI 或运行时二进制兼容承诺。
- **方向**：CPU-first 构建一条能验证真实性能和精度的图执行 / FlowUnit 模型；后续才扩展真实流式视频、RK3588/NPU、Flow Studio、SDK 和插件。
- **workspace**：`crates/modelbox-core`（Graph、Runtime、FlowUnit、Tensor、可选 ONNX CPU），`crates/modelbox-cli`（测试、推理、benchmark CLI）；Rust + serde + crossbeam-channel + ONNX Runtime CPU（`--features onnx` opt-in）；JPEG/PNG 通过 `image=0.24.9`。
- **Graph**：原生 JSON `GraphSpec`，有 DAG、静态端口、强类型验证、唯一 Source、有限容量队列；Worker 每节点一个；输入按 sequence 关联；fanout 和 terminal 保留序号；错误与取消协调。**不是持久运行的摄像头/RTSP Graph。**
- **`Value`**：`Text/Integer/Boolean/Bytes/TensorF32`，Tensor 为拥有所有权的连续 CPU f32 `Vec<f32>`。没有显卡/NPU 设备内存协议。
- **`FlowUnit`**：`process(&[Value])` + 可覆写的 `process_owned(Vec<Value>)`。单消费者 Channel 和输出走 Move，额外 fanout 按需 Clone；这是限定情形的数据搬运优化，**不代表所有执行路径无拷贝**。
- **每次 `Runtime::run_plan`**：构造 Capture / Mutex / cancel flag / bounded channels，使用 `std::thread::scope` 为每个节点启动 Worker，发送数据，等待 join，排序/回收结果。单帧短图形执行上的额外开销是当前性能研究重点。
- **ONNX 适配器限制**：`onnx.cpu` 目前只接受一个 f32 输入张量和一个 f32 输出张量；尚无通用动态形状、多 IO、多 dtype、NPU Provider 或 SSD 多输出支持。
- **取消限制**：合作式取消，第三方 Unit 在 `process` 中无限阻塞不能在进程内保证强制停止；需要 IPC/隔离/监督机制时另设计。

## 2. 截至 PR #24，哪些事情已经真正完成

| PR | 已合并内容 | 证据/解释 |
|---|---|---|
| [#12](https://github.com/hu-qi/modelbox-rs/pull/12)、[#13](https://github.com/hu-qi/modelbox-rs/pull/13) | Owned 传输、默认 `process_owned`、单读者 Move，terminal 不重复 Clone | 检测 fanout 例外，Core/CI |
| [#14](https://github.com/hu-qi/modelbox-rs/pull/14)～[#16](https://github.com/hu-qi/modelbox-rs/pull/16) | 预训练 MNIST CNN ONNX、PNG、带标签 10 图 CSV 验收 | 真实文件，不再只是合成 relu |
| [#17](https://github.com/hu-qi/modelbox-rs/pull/17) | 官方 IDX 测试集 **10,000 张**，数字混淆矩阵 | **9,890/10,000 = 98.90%**，Linux 与 Mac arm64 一致，CI 允许下限 9,850 |
| [#18](https://github.com/hu-qi/modelbox-rs/pull/18) | 纯 Rust YOLOv8/YOLO11 raw NMS-free tensor 后处理 | 仅 `[1,C,N]`（NMS-free / 已激活类别置信度），不是通用 YOLOv5/Segmentation |
| [#19](https://github.com/hu-qi/modelbox-rs/pull/19) | 真实 YOLOv8n pretrained ONNX + JPEG/PNG RGB / Letterbox / CHW / Graph ORT / NMS / 原图坐标 | 真实 `bus.jpg`，**4 人 + 1 bus，共 5 框** |
| [#20](https://github.com/hu-qi/modelbox-rs/pull/20) | 实机五框 Mac golden，跨 OS 数值回归 | 对比五框的类别/坐标/score，坐标容差 8 px、confidence 0.03；非独立精度标注 |
| [#21](https://github.com/hu-qi/modelbox-rs/pull/21) | 完整文件到 YOLO 框的分阶段计时 | 会话/Graph 编译外置；四阶段时间严格闭合 |
| [#22](https://github.com/hu-qi/modelbox-rs/pull/22) | Mac Release 性能基线归档；同 Session Graph vs Direct 对照 CLI | 相同权重、输入、Session；输出逐元素检查 |
| [#23](https://github.com/hu-qi/modelbox-rs/pull/23) | 归档 Mac Graph/Direct 配对结果，`bench-transport-yolov8` 4.9152MB 无模型 Identity Graph 控制 | **CLI 已在 main**，但 Mac Release 控制实验结果待上传 |
| [#24](https://github.com/hu-qi/modelbox-rs/pull/24) | 验证 Owned Graph/Direct identity 前后**buffer pointer 不变**，增加 Core/CLI 回归 | 只证明此单消费者 identity 路由 Buffer 无额外复制 |

- GitHub 最后核验主分支提交：`22f0e129270b4c7fcd2ae402fc292678ada25582`。
- 已核验 [main CI 38107754576](https://github.com/hu-qi/modelbox-rs/actions/runs/38107754576)：**5/5 succeeded**，包括 Ubuntu、macOS、Windows Core，Linux x86_64 + **原生 macOS ARM64** 的 ONNX 测试。Windows ONNX 后端并未因此获得验证。
- 核验时**没有 Open PR**；旧分支即使还存在，也不要当作未合并新任务。

## 3. Mac mini 已有真实观测（不要推测硬件 SoC 型号）

### 3.1 完整 YOLOv8n：同图顺序请求，30 次测量 / 5 次预热

命令：

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-detect-yolov8 \
  benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 30 5
```

| 互斥计时阶段 | Mean | P50 | P95 | Mean 占比 |
|---|---:|---:|---:|---:|
| `decode_file_and_rgb` | **4.4716569 ms** | 4.465500 ms | 4.577583 ms | 15.46% |
| `letterbox_and_chw` | **4.1538084 ms** | 4.147084 ms | 4.234042 ms | 14.36% |
| `graph_including_onnx_cpu` | **19.8085583 ms** | 19.657542 ms | 20.963459 ms | 68.49% |
| `nms_and_project_boxes` | **0.4868138 ms** | 0.486083 ms | 0.509000 ms | 1.68% |
| **`end_to_end`** | **28.9208374 ms** | **28.680000 ms** | **30.320666 ms** | 100% |

- 四阶段时间严格闭合：`max_stage_total_reconciliation_delta_ns=0`，`mean_stage_sum_ms=28.920837366666667`。
- `estimated_sequential_images_per_second_from_mean=34.57714544436295` 是**从均值倒数估算**，不是持续摄像头 FPS；有 OS 缓存，每次解码同一图片。
- 最后检测 `last_detection_count=5`、`last_detection_numeric_fingerprint=9498.732394576073`。
- **下载模型 42 秒、第一次 Cargo 编译 43 秒、后续 Cargo 增量编译 0.2～2.6 秒等，全都不是推理延迟。**
- 存档：[Mac Release 阶段基线](../benchmarks/baselines/2026-10-11-macos-arm64-yolov8n-release-stages.json)。

### 3.2 同一 ONNX Session 的 Graph vs Direct：21 组交替配对 / 5 次预热

命令：

```bash
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare-yolov8 \
  benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 21 5
```

| 指标 | Direct FlowUnit | 3-node Graph |
|---|---:|---:|
| Mean | **18.7113531 ms** | **20.2150278 ms** |
| P50 | 18.661875 ms | 20.265500 ms |
| P95 | 19.344042 ms | 20.968875 ms |
| Max | 19.512375 ms | 21.042084 ms |

- **Graph－Direct Mean = +1.5036746666666687 ms（8.03616%）**；21 组差值全为正（+0.45425～+2.523126 ms，差值中位数 +1.478583 ms）。
- 同 Session、同一预处理张量，`max_output_element_abs_diff=0.0`，输出 `[1,84,8400]` 共 **705,600 个 f32**；最后检测 5 框，fingerprint 与 3.1 一致。
- 这一数值不是**纯 Graph Scheduler 开销**：相比 Direct，还包括 Graph 执行路径差异，如输入/输出封送、Worker 生成/销毁、队列、Capture，且单次配对样本受热状态影响。
- 存档：[Mac Graph vs Direct 21 组基线](../benchmarks/baselines/2026-10-11-macos-arm64-yolov8n-graph-vs-direct-release.json)。
- **本次仍没有 Mac mini 的 `bench-transport-yolov8` Release 实测结果。任何未来窗口直接声称其均值都属于虚构。**

### 3.3 MNIST 全量、YOLO 真图验收边界

- MNIST 真正有官方 10,000 图测试标签：9890 个正确、110 个错误，98.90% 是**该固定模型/该测试集**上的分类准确率；不是生产场景推广准确率。
- YOLO 只有真实照片的功能确认+五框数值 golden，**没有**独立标注的 mAP、模型泛化指标、独立后处理 reference boxes。
- YOLO 布局是 `[1,84,8400]` raw no-NMS / 80 类，每 anchor 单类别，阈值 confidence 0.25、IoU 0.45、最多 300 框，候选截断 top3k；输入 `[1,3,640,640]`，RGB /255、114 padding、Triangle resize。
- 真实图 `bus.jpg` 原图 **810×1080**，gain 约 **0.5925926**，缩放 480×640，左 padding 80，输出 xyxy 在**原始图像坐标**。
- 原实机五框 `person×4,bus×1`，模型最高 person confidence 0.8959175、bus confidence 0.8366719，数值校验曾得到本机坐标/score 双 0 偏差。
- golden：[五框固定观测](../benchmarks/baselines/2026-10-11-macos-arm64-yolov8n-bus-observed.json)；验证器 `scripts/verify-yolov8n-demo.py`。

## 4. 固定外部资源、许可与复现

- ONNX：`https://huggingface.co/webml/yolov8n/resolve/main/onnx/yolov8n.onnx`，SHA256 `190ba5f1e61411a001683e349d6b2cdb0804c0dc67a5e34cd8ff6fd00ee54b4d`。权重存在 **AGPL-3.0** 义务；不可擅自认为商用授权已获批准。
- 照片：`https://raw.githubusercontent.com/ultralytics/assets/main/im/bus.jpg`，Git blob SHA1 `40eaaf5c330d0c498fbe1dcacf9bb8bf566797fe`；第三方资产许可与发布再分发需单独审核。
- 统一下载及校验：`bash scripts/fetch-yolov8n-demo.sh`；资产位于 Git ignore 的 `benchmarks/datasets/detection/`，**不提交模型二进制或照片**。
- MNIST 官方测试集：`bash scripts/fetch-mnist-test.sh`，官方下载的 IDX 图片/标签按脚本 SHA256 验证，位于 `benchmarks/datasets/mnist/`；固定 MNIST ONNX `examples/mnist.onnx`。
- `image::FilterType::Triangle` 和 Ultralytics OpenCV 线性插值可能导致输入像素与盒坐标差异。不能把本项目自己算出的 five-box golden 误认作参考实现的 ground truth。

## 5. 新窗口先看的源文件索引

| 文件 | 作用 |
|---|---|
| [docs/TODO.md](TODO.md) | 首要待办、各项验收标准、禁止的推断 |
| [docs/roadmap.md](roadmap.md) | v0.1～v0.5 方向与退出门槛 |
| [docs/architecture.md](architecture.md) | Core DAG / FlowUnit 的初版设计与约束 |
| [docs/object-detection.md](object-detection.md) | YOLO 数据布局、NMS、Letterbox 与精度工作 |
| [crates/modelbox-core/src/runtime.rs](../crates/modelbox-core/src/runtime.rs) | 通道、Worker、Capture、`run_plan` |
| [crates/modelbox-core/src/flow_unit.rs](../crates/modelbox-core/src/flow_unit.rs) | `process` / `process_owned` 与内置 Unit |
| [crates/modelbox-core/src/value.rs](../crates/modelbox-core/src/value.rs) | 拥有所有权的 `TensorF32` 与可复制边界 |
| [crates/modelbox-cli/src/yolo_infer.rs](../crates/modelbox-cli/src/yolo_infer.rs) | JPEG/PNG → RGB/Letterbox/CHW → YOLO Graph → box 投影 |
| [crates/modelbox-cli/src/yolo_post.rs](../crates/modelbox-cli/src/yolo_post.rs) | 校验 raw tensor、过滤、class-aware NMS |
| [crates/modelbox-cli/src/bench_yolo.rs](../crates/modelbox-cli/src/bench_yolo.rs) | 完整 file→detection 分段计时 |
| [crates/modelbox-cli/src/bench_yolo_compare.rs](../crates/modelbox-cli/src/bench_yolo_compare.rs) | 同 Session ONNX Direct/Graph 21 配对 |
| [crates/modelbox-cli/src/bench_yolo_transport.rs](../crates/modelbox-cli/src/bench_yolo_transport.rs) | **下一项**：4.9152MB Owned Identity Graph vs Direct 控制 / pointer 同一性 |
| [benchmarks/README.md](../benchmarks/README.md) | 基线方法论、原始数据位置和科学解释 |
| [.github/workflows/ci.yml](../.github/workflows/ci.yml) | 5 Job 的 Gate（跨 OS 和两个 ONNX CPU 目标） |

## 6. CLI 复现速查

```bash
git pull origin main
bash scripts/fetch-yolov8n-demo.sh

# 第一优先级：未采集的 Mac Release Identity Graph transport
cargo run --release -p modelbox-cli --features onnx -- \
  bench-transport-yolov8 benchmarks/datasets/detection/bus.jpg 21 5 \
  > yolo-transport-release.json

# 之前已采集的完整四阶段基线（可作回归）
cargo run --release -p modelbox-cli --features onnx -- \
  bench-detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 30 5

# 之前已采集的同 Session Graph/Direct 配对（可复测）
cargo run --release -p modelbox-cli --features onnx -- \
  bench-compare-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 21 5

# 真实照片功能检测 & 观测 golden
cargo run --release -p modelbox-cli --features onnx -- \
  detect-yolov8 benchmarks/datasets/detection/yolov8n.onnx \
  benchmarks/datasets/detection/bus.jpg 0.25 0.45 > yolo-detections.json
python3 scripts/verify-yolov8n-demo.py yolo-detections.json

# 10,000 张 MNIST 分类回归
bash scripts/fetch-mnist-test.sh
cargo run --release -p modelbox-cli --features onnx -- \
  eval-mnist-idx examples/mnist.onnx \
  benchmarks/datasets/mnist/t10k-images-idx3-ubyte \
  benchmarks/datasets/mnist/t10k-labels-idx1-ubyte 9850
```

## 7. 与用户的协作约定

- 用户使用 Apple Silicon **Mac mini**，可以在本机执行 Rust Release/ONNX CLI；偏好**真正操作已连接的 GitHub**，而不是仅给理论操作指令。
- 允许直接推进小粒度代码 / 文档变更：`feature branch → PR → 自动 CI 修复 → 5/5 green → squash merge → main 5/5`；若 CI 失败，先修正，**不得**为了进度强合并。
- Mac 本机新数据只能来自用户提供的真实输出；GitHub 云端 CI 的 Debug 模式只用于功能、格式和跨平台兼容，不能当成用户设备的 Release 性能。
- 保留可回放 fixture、精度/数值验证、输入输出形状、阈值、模型 hash、编译配置和严格限定结论。
- 当前核心问题不是“能不能跑真实模型”（已完成），而是**为什么 Graph 相对 Direct 多约 1.5 ms，优化后是否能显著改善 28.92 ms 文件到框总延迟，同时不破坏通用 DAG**。
- 如果已知上下文不足，优先看 GitHub 真实代码/CI 和本文档；不要要求用户重新解释已存在的全部项目背景。
