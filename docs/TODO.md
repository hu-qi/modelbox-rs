# ModelBox-RS TODO（新窗口续接的权威任务清单）

> 更新基点：2026-10-11；已核实的 main 提交 `22f0e129270b4c7fcd2ae402fc292678ada25582`（PR #24）。  
> 项目：<https://github.com/hu-qi/modelbox-rs>。完整背景与复现指令参见 [NEXT-SESSION-CONTEXT.md](NEXT-SESSION-CONTEXT.md)。  
> **规则**：`[x]` 表示有明确的代码/CI/本机证据；`[ ]` 表示尚未验收。未测出的性能、精度和硬件能力一律不得声称已实现。清单按依赖顺序排列，不代表承诺交付日期。

## P0 — 立即推进：真实 Mac mini 性能归因（下一窗口第一轮）

- [ ] **P0-01｜采集同尺寸、无模型的 Graph 传输 Release 基准**
  - 在已更新的 Mac mini 上运行：
    ```bash
    git pull origin main
    cargo run --release -p modelbox-cli --features onnx -- \
      bench-transport-yolov8 benchmarks/datasets/detection/bus.jpg 21 5 \
      > yolo-transport-release.json
    cat yolo-transport-release.json
    ```
  - 核对 `profile=release`、`input_tensor_bytes=4915200`、`measured_pairs=21`、`warmup_pairs=5`、`onnx_model_loaded=false`、`output_matches_input_exactly=true`、`graph_tensor_buffer_reused=true` 和 `direct_tensor_buffer_reused=true`。
  - **验收**：保存实际 min/mean/p50/p95/max、21 组配对差值；归档为有来源的 `benchmarks/baselines/2026-10-11-macos-arm64-yolov8n-owned-transport-release.json`（如实使用测量日期），记录 CPU SoC、macOS、`rustc -Vv`、电源/负载环境等**实际获得的**信息；不得杜撰缺失字段。
  - **解释边界**：这是没有 ONNX 算子的 3 节点 `tensor.input → bench.tensor_identity → tensor.output` 控制，不能直接从真实模型的 1.5037 ms 差距中相减，声称得到“纯调度”开销。

- [ ] **P0-02｜复测 Graph 与 Direct 的 21 组同 Session 配对**
  - ```bash
    cargo run --release -p modelbox-cli --features onnx -- \
      bench-compare-yolov8 \
      benchmarks/datasets/detection/yolov8n.onnx \
      benchmarks/datasets/detection/bus.jpg 21 5
    ```
  - 推荐在空闲/相同电源状态下做多次独立运行；分别保存时间序列与统计值，避免拿 Debug CI 性能当作 Mac Release 数据。
  - **验收**：完整输出 `[1,84,8400]` 的数值差 `max_output_element_abs_diff <= 1e-4`（既有观测为 0）、`last_detection_count=5`；报告均值与 P50/P95、配对差值分布、重复运行方差。不设没有证据的目标 FPS。

- [ ] **P0-03｜为 Runtime::run_plan 设计低侵入式诊断计时**
  - 查看 `crates/modelbox-core/src/runtime.rs` 的每次执行：类型检查 → 终端 Capture 初始化 → bounded channels 创建 → 各 Node Worker 构造 → `std::thread::scope` spawn/execute/join → 捕获排序/结果构建。
  - 用可选诊断模式/feature 或带独立计时的 benchmark，区分阶段；**确保 instrumentation 有开销且可开关**，并保证不破坏通用 DAG、取消、错误传播、顺序、背压。
  - 不要先假设 1.50 ms 全来自 thread spawn：`Value` 输入复制位置、Channel/Fanout、捕获分配、ORT 数据封送均需分别核实。
  - **验收**：可重跑的实证诊断结果，明示哪些数值互斥可加、哪些异步重叠不可相加。

- [ ] **P0-04｜只对证实的瓶颈做最小可回滚优化**
  - 根据 P0-01～03 才选择：Worker 复用、一次性单帧执行专用路径、Channels/Captures 复用、缓冲区复用，或先优化已有 8.63 ms 的 JPEG+Letterbox。
  - 任何 Graph 快速路径必须有**等价的输入/输出/错误/取消行为**，不能通过关闭类型验证、丢帧、取消超时检测或绕过 Graph 来虚报加速。
  - **验收**：PR 同时包含优化前后在相同 Mac mini 的 Release A/B 对照与回归；必须保留 MNIST 10,000 图、YOLO 5 框、Graph/Direct 数值一致性、fan-out/fan-in / cancellation / channel 测试。

- [ ] **P0-05｜补测端到端变化与模型加载开销**
  - 复跑 `bench-detect-yolov8 ... 30 5`，对照基线 28.9208 ms 总均值和各阶段耗时，观察优化是不是实际改善了文件到框的时间。
  - 另行统计 cold start：磁盘加载、ORT Session 建立、Graph 编译；**不要混入已预热单张延迟**。
  - 如果将目标改成视频/FPS，需另建真实持续流入的数据源与稳态吞吐测试，不能把 `1000 / mean_ms` 称为实测实时帧率。

## P1 — 独立推理正确性与检测精度

- [ ] **P1-01｜独立 reference 推理对照**
  - 固定相同 SHA256 的 YOLOv8n ONNX、同一真实照片、RGB/CHW/255、原始 810×1080 画布与 resize/letterbox 参数。
  - 使用独立的 Python/Ultralytics 或参考工具链，分别固定**原始 ONNX logits/boxes**和**post-NMS class/score/原图 xyxy**；清楚说明若双方共用 ORT，它只是独立前后处理实现，**不是独立 ONNX 内核算子 oracle**。
  - 鉴于当前 Rust `image::FilterType::Triangle` 与 OpenCV `INTER_LINEAR` 可能存在差异，需要做像素级输入误差与 box/score 容差分析，不能默认逐元素相等。
  - **验收**：源码、版本、许可证、模型/图像 SHA、环境和阈值可追踪；对 score、box、class 的数值差异有自动化报告；无隐式放宽阈值。

- [ ] **P1-02｜带标注的数据集与 mAP**
  - 选择许可和再分发范围清楚的标注子集；校验下载 SHA、类别映射、图像数、标注格式。
  - 实现正确的 TP/FP、按置信度排序、IoU 0.50 与 0.50:0.95、类/图像边界行为；提供小型人工可核对单元用例。
  - 输出 precision/recall、mAP@0.5 和 mAP@0.5:0.95，并与有同一模型/预处理设置的独立工具对照。
  - **验收**：完整标注与评价脚本，确定的 CI smoke + 可重跑的完整评估；一张 `bus.jpg` 上识别了 5 个目标**不构成** mAP 证明。

- [ ] **P1-03｜图像前后处理鲁棒性**
  - 测试非方形、奇数边长、极宽图、高分辨率、无目标、阈值边界、Letterbox 奇数 padding、RGB/BGR、框裁剪、空输出、恶意损坏文件。
  - 明确像素插值差异、原图坐标取整策略、COCO80 类别表、Raw YOLOv8 单输出数据布局。
  - **验收**：代码与 fixture 覆盖异常路径，类型/内存边界保持安全；支持格式只限实际验证的范围。

- [ ] **P1-04｜真实模型合同与授权**
  - 当前演示模型来自 `webml/yolov8n`；模型权重有 **AGPL-3.0** 合规义务，`bus.jpg` 为外部第三方素材。
  - 文档应持续明确第三方来源、SHA、不可擅自当成商用授权、不随仓库 vendoring 模型。
  - **验收**：许可证核查清单及发布合规说明，避免“可下载”被写成“可自由商用”。

## P2 — Runtime / FlowUnit 真正产品化

- [ ] **P2-01｜流式 API**：从有限批次 `run_plan(Vec<Value>)` 进化为可持续 ingest、bounded ingress/egress、显式完成/关闭、frame ID、Backpressure 的流式接口。先补取消与停止生命周期用例。
- [ ] **P2-02｜Worker 生命周期与资源回收**：可选 Worker 复用或池化；验证长期运行、丢帧、下游断开、异常中止、循环启停、无死锁；第三方 FlowUnit 的无限阻塞需要隔离策略，不能承诺进程内强制取消。
- [ ] **P2-03｜ONNX FlowUnit 能力边界**：当前仅**一进一出、f32**；待扩展多输入/输出、动态 shape、其他 dtype、设备 Provider，所有扩展必须有模型和跨平台测试。不要先声称支持 SSD/Segmentation 等多输出模型。
- [ ] **P2-04｜内存与可观测性**：分配次数、常驻内存、输出缓存、Worker/Channel 各阶段时序、结构化 Trace、错误上下文和可复现的 CI/本机 profiling。
- [ ] **P2-05｜版本/兼容**：为原生 Graph JSON Schema、可选的 DOT/TOML 读取、FlowUnit Rust API、未来 C ABI 设计清楚的版本边界；**当前不具备**原 ModelBox C++ ABI/plugin 二进制兼容能力。
- [ ] **P2-06｜构建/CI 保障**：继续保持 Ubuntu/macOS/Windows Core Job 与 Linux/native Apple Silicon ONNX Job **5/5**；单独保护 Release smoke、Rustfmt、Clippy、测试，留存测量基线；审核依赖变更和第三方许可。

## P3 — v0.3+ 中长期方向（未开始，不在当前阶段冒称实现）

- [ ] **v0.3 Edge AI**：RK3588 / RKNN（版本固定的厂商 SDK）、GStreamer/FFmpeg / RTSP、设备内存所有权、重连、恢复与长时间 soak、Watchdog。
- [ ] **v0.4 Flow Studio**：可视化 DAG 编辑、参数校验、端到端运行、日志追踪、导入导出；原 ModelBox Graphviz-DOT-in-TOML 只规划先读兼容层。
- [ ] **v0.5 生态**：Python SDK、稳定版本化 C ABI、TypeScript 控制端、插件打包/签名、Docs 站点、Release/tag 自动化。
- [ ] **首个版本发布**：仓库 CI 成功已确认，但发布 tag、API 兼容声明、许可证审计、产物交付矩阵仍要实际处理。

## 已完成并保留的硬证据（不要重复开票）

- [x] **PR #12–#13**：单消费者通道和终端输出的 Owned Value 转发/复用；多消费者扇出额外复制有明确边界。
- [x] **PR #14–#17**：真实 MNIST CNN、PNG 输入、10 图有标签测试、完整官方 **10,000 图**验收；9,890/10,000 = **98.90%**，Linux 和 Apple Silicon 输出一致，CI 最小正确数 **9,850**。
- [x] **PR #18–#20**：YOLOv8 raw [1,84,8400] 适配器、NMS、真实 pretrained ONNX + `bus.jpg`；检测 **4 人、1 辆 bus**；五框观测基线与 Linux/Mac 数值回归（框 ±8 像素、confidence ±0.03）。
- [x] **PR #21**：4 阶段 Release 计时与时间闭合；Mac 总均值 **28.920837 ms**，P50 **28.68 ms**，P95 **30.320666 ms**；**不是实测视频 FPS**。
- [x] **PR #22–#23**：同 ONNX Session Graph/Direct 配对实验，Mac `direct=18.711353 ms`、`graph=20.215028 ms`，差 **1.503675 ms（8.04%）**，21 个差值全为正，模型输出最大元素差 0；建立 4.9152 MB 无模型传输控制。
- [x] **PR #24**：对 Owned Graph/Direct identity 加入 **buffer pointer 不变**断言及单元测试，证明该控制路径不会悄悄复制大输入；**不代表 ORT 内部零拷贝**。
- [x] 已核对 `main@22f0e129270b`、GitHub Actions run `38107754576`：跨平台 **5/5** 全部成功。

## 新窗口开发协作规则 / Definition of Done

1. **先读** [NEXT-SESSION-CONTEXT.md](NEXT-SESSION-CONTEXT.md)、本文件、`docs/roadmap.md`、`benchmarks/README.md`、相关源文件；以 GitHub 当前 `main` 最新状态为准，防止覆盖其他已合并的 PR。
2. 以仓库已验证的模型/图片 SHA、指标定义和已存基线为事实来源；对“用户本机新性能数据”必须收到新的输出再记录。
3. 功能改动使用**特性分支 → PR → 修复 CI → 全部绿后合并 → main 再验收**；不要合并未绿的 PR。
4. 对每项优化保留负例测试与数值等价对照；区分 **smoke / 数值回归 / 独立 accuracy / 性能 / 稳态吞吐**。
5. 不要将 `cargo Finished in …` 的编译耗时当推理延迟，不要把 `1000/mean` 当摄像头 FPS。
6. 本项目仍是 **CPU-first Rust 原型**，目标是逐步实现可交付的数据流 AI Runtime；不是已与原 C++ ModelBox 全功能或 ABI 等价的替代品。
