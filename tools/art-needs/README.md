# art-needs — 游戏仓库 → 岩库（ArtVault）的美术需求提报桥

> 决策依据：团队 `docs/decisions/0015-art-needs-demand-integration.md`（ADR-0015）；
> 岩库侧对应其 ADR-006（需求板：Demand 实体 + 状态机）。
> 一句话：**地图是内容真源，需求是投影——地图改了，需求清单跟着变。**

补 ADR-0009 资产流的上游缺口：以前「游戏要什么美术」靠聊天记录，现在声明在
`<game>/docs/art-needs.json`（进 git，规格可评审），状态在岩库需求板
（draft → submitted → accepted → producing → review → fulfilled → locked）。

## 快速开始

```bash
# 前置：本仓库根目录（要能跑 cargo）；submit/status 需要岩库服务在跑
python3 tools/art-needs/art_needs.py games/loop-td scan           # 扫图 diff 报告（纯本地）
python3 tools/art-needs/art_needs.py games/loop-td scan --write   # 把新需求草稿写进文件
python3 tools/art-needs/art_needs.py games/loop-td submit         # 草稿提报岩库 + 回填 demand_id
python3 tools/art-needs/art_needs.py games/loop-td status         # 拉岩库状态投影 + 覆盖度
python3 tools/art-needs/art_needs.py games/loop-td sync           # 锁定上报（锁清单 demand 字段 → 岩库 locks）
```

退出码 = 问题数（0 = 干净），与 art-sync / art-catalog 同约定。
零依赖（Python 3 stdlib only），人跟 AI 跑同一条命令。

## 工作原理

1. **扫描**：`scan` 调 warbase 的 `mapbundle-dump` bin
   （`cargo run -p warbase --bin mapbundle-dump -- <map.ron>`）——RON 走 serde
   真反序列化 + bundle 校验后输出 JSON，**不做文本猜测解析**（格式演化 =
   serde 报错 = 坏图警报，投影永不撒谎）。
2. **分组**：`upgrade_to` 升级链合并为一条需求（如 arrow/arrow2/arrow3 =
   一套底模三阶变体，`source_defs` 列全链）；功能塔/英雄塔/怪每 def 一条。
3. **diff**：新增 def → 新草稿（spec 模板预填，人再改）；def 消失 → 标记
   `removed`；已提报条目的 def 集变化 → 提示走岩库 `changed` 流转（v2 迭代环）。
4. **提报**：`submit` 把 draft 条目 POST 岩库 `/demands` 并流转 submitted，
   幂等（已有 demand_id 的跳过）；提报时顺带打印**复用提示**（岩库相似资产）。
5. **投影与上报**：`status` 把岩库状态拉回本地（fulfilled 资产 id → 提示可进
   art-mapping / art-assets；fulfilled-无关联资产等失真状态会报问题——岩库零校验，
   质量责任在游戏侧投影）；`sync` 读 art-assets.json 的 demand 回链字段上报锁定
   （DemandLock 记录，幂等 upsert，版本升级后重跑即刷新）。

状态语义（契约 v1，已会签）：岩库对状态流转**零校验**（PATCH 直改 + 事件留痕），
约定工作流由本 CLI 与 P2 看板引导；**locked 是记录不是状态**——锁定事实源在
锁清单，岩库被动接收上报，与需求生命周期正交。

## 文件契约（三文件，全部进 git）

### `<game>/docs/art-needs.json`（需求清单，规格权威在本仓库）

```jsonc
{
  "schema_version": 1,
  "game": "loop-td",
  "source": "http://127.0.0.1:8765",     // 岩库地址（同 art-sync 安全校验）
  "source_maps": ["maps/loop.ron"],       // 需求源头地图
  "demands": [
    {
      "local_key": "tower-arrow",         // 本地稳定键（scan 生成）
      "origin": "scan",                   // scan 生成 | manual 手工条目（如地形）
      "name": "箭塔塔线 arrow",             // 提报名称——按需改，别用默认
      "category": "model",
      "source_defs": ["arrow", "arrow2", "arrow3"],
      "spec": { "poly_budget": "≤ 12k tris/阶", "animations": ["idle", "attack"], ... },
      "acceptance": "三角面 ≤ 12k、动画齐全、showroom turntable 过审",
      "priority": "P1",
      "demand_id": null,                  // submit 后回填（岩库 id）
      "status": "draft",                  // 本地投影，status 命令更新
      "removed": false
    }
  ]
}
```

### `<game>/art-mapping.json`（换装映射，ART-L2 数据面）

`def_id → "岩库:<asset_id>[@vN]"` 或 `null`（灰盒占位——需求未 fulfilled 的
默认态，内容不被美术阻塞）。demand fulfilled 的落点就是这里多一条可用项。

### `<game>/art-assets.json`（锁清单，ADR-0009 不变）

条目新增可选字段 `demand`（demand_id）——「需求 → 资产 → 锁定版本」三层
可追溯；art-sync 对它做只读校验（指向的 demand 必须存在）。

## 标准工作流

- **新图/新单位**：改 `maps/*.ron` → `scan --write` → review 生成的草稿
  （改 name/spec/acceptance/priority）→ 提交 → `submit`。
- **需求变更**（塔三阶化、改动画清单）：改地图 → `scan` 报 changed →
  若已提报：岩库流转 `changed`（留痕）→ 本地改 spec → `submit` 重报。
- **资产入库**：岩库 fulfilled 后 → `status` 看到提示 → `art-mapping.json`
  配 def → `art-assets.json` 锁版本（demand 字段回链）→ `art_sync.py` 拉文件。

## 安全边界与解耦红线（工具内置）

- `source` 仅允许环回/私网/链路本地地址（与 art-sync 同策略）；禁跟随重定向。
- **管理流与构建流解耦**：岩库宕机 → `scan` 照常（纯本地）、`submit/status`
  报告并以非零退出；任何游戏构建/测试不依赖岩库在线。

## 测试

```bash
python3 -m unittest discover tools/art-needs   # 纯函数：分组/diff/模板
cargo test -p warbase --test art_needs_dump    # Rust 侧投影契约
```
