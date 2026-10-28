# assets-art-drill — 美术首产日全环彩排（round-4 DRILL）演练场

> **DRILL 专用目录约定**：本目录及其全部子目录是 round-4 DRILL 增量
> （warbase-round4-plan §Phase 2）的演练物，**正式管线永不读取这里**。
> 与正式资产的隔离靠三层：①独立目录（不在 `assets/` 树内，bevy 的
> asset loader 扫不到）；②文件名一律 `drill_` 前缀；③引用了本目录的
> 唯一入口是演练测试（`tests/art_drill.rs`，全部 `#[ignore]` 文件门控）
> 与 `--art --art-manifest` 显式传参。gitignore 不忽略本目录——演练
> 证据链（贴图/清单/校验输出/取证图）需要随仓库提交评审。

## 这是什么

用**程序化生成的演练贴图**把「美术首产日」的全环走了一遍
（跨项目工作流 C 验收同步 + D 升级回环）：

```
make_drill_textures.py          程序化生成演练贴图（模拟美术在岩库交付）
vault/                          模拟岩库存放点：drill PNG × 5 + vault_db.json
mock_vault.py                   模拟岩库 REST API（127.0.0.1:8799，只给演练用）
first-day/                      演练用「伪游戏根」——art-sync 的 <game> 参数：
  docs/art-needs.json             需求投影（2 条演练卡，demand_id 已回填）
  art-assets.json                 演练锁清单（demand 回链，ADR-0009 格式）
  assets/ui/                      art-sync 落点 + uitextures.ron（--art 入口）
```

逐步命令与每步产物见 **`games/loop-td/docs/art-first-day-runbook.md`**
（手册正文 + 演练全程实录附录 + 流程缺陷清单 F1-F8）。

## 重演一遍（约 5 分钟）

```bash
# 1. 生成演练贴图 + 岩库数据表（确定性输出，重跑零漂移）
python3 games/loop-td/assets-art-drill/make_drill_textures.py

# 2. 起模拟岩库（Ctrl-C 停）
python3 games/loop-td/assets-art-drill/mock_vault.py &

# 3. 照 runbook「演练实录附录」逐条走（status → mapping → 锁清单 →
#    art-sync → --art → 升级 → sync 上报）

# 4. 演练测试（文件门控 + GPU 门控，均须 --ignored 显式触发）
cargo test -p loop-td --test art_drill -- --ignored --nocapture
```

## 为什么 mock 岩库

art-sync 硬依赖岩库 REST 在线（无本地校验模式，见其 README 前置节），
而演练要求可离线重放 → 用只读 mock 顶替岩库侧（缺陷 F8：建议 art-sync
补 `--offline` 本地字节校验模式）。mock 只实现 art-sync / art-needs 消费
的端点形状，锁状态存内存不落盘——它不是岩库的替代品，是彩排的道具。
