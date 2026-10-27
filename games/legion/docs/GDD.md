# GDD — legion（军团战争图，多人对抗 2v2）

> 立项日期：2026-10-28。基于 Bevy 0.19 + warbase 基底（ADR-0012）。
> 本文件是**游戏设计文档**：只放本游戏自己的设计。数值锚点源：vault
> `docs/topics/game-design/legion-td-proposal.md`（v0.1）。

## 一句话玩法

四名玩家（P0P2 vs P1P3）各守一路：建造期花钱雇兵布到自家 12 格布阵区，
战斗期雇佣兵沿路自动攻打对家，漏怪扣命、命尽出局、LastTeamStanding 定胜负。

## 核心循环

```
建造期 30s（帧计 1800）买兵布阵 → 战斗期：全员 AttackMove 至对家底线
→ 路上自动交战（基底 combat）→ 漏怪：抵达对家底线者对家命 -1
→ 战场清空 → 结算：+50 金 + 击杀赏金（def bounty）→ 下一回合
```

## 数值表（起点值，全部可调；正式锚点以 vault proposal §2 为准）

| 项 | 值 |
|---|---|
| 玩家 | 4 位，2v2（teams 段：P0P2 队0 vs P1P3 队1） |
| 布阵区 | 每路 12 格（FixedSlots，4 列 x 3 行，间距 1.0） |
| 雇佣兵 | 6 种：cheap 30g/人口1 · swarm 45g/2 · tank 120g/3 · ranged 70g/2 · aoe 100g/3 · support 60g/2 |
| 起始金 | 每家 120（econ_start.gold） |
| 人口上限 | 每家 15（econ_start.pop_cap，游戏侧 per-player 记账） |
| 回合收入 | 50 基础 + 击杀赏金（def bounty_gold） |
| 命 | 每家 20，漏 1 扣 1，扣尽出局 |
| 回合数 | 20 封顶（LG2 落地） |
| 发兵 | 25 金起，同目标 +10%（LG2 落地） |

## 范围

**做**（LG0-LG3）：回合层状态机、雇佣兵商店、自动交战、漏怪扣命、收入结算、
发兵、方案 A 灰盒 UI。

**不做**：netcode、AI 对手、贴图美术（proposal §4）。

## 洞清单（通道 C，基底缝缺失 → 游戏侧补丁孵化）

见 [kpi.md](kpi.md)「新洞」节——每增量滚动记录。
