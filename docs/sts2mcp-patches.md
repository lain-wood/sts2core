# STS2MCP 的四个本地补丁

上游 mod 是 [Gennadiyev/STS2MCP](https://github.com/Gennadiyev/STS2MCP)（MIT），
本地 clone 在 `D:\game mod\STS2MCP\`，基于 `55e0648`。四个补丁都打在
`McpMod.StateBuilder.cs` 上，**全文在 [`../patches/sts2mcp-statebuilder.patch`](../patches/sts2mcp-statebuilder.patch)**。

装着的 DLL 已经打过补丁，打补丁之前的 DLL 备份在 `mods/STS2_MCP.dll.bak-before-*`。

> **2026-09-19 之前，这四个补丁只存在于 STS2MCP 的未提交工作区里** ——
> 一次 `git checkout` 就会全部丢失。现在 `patches/` 里有一份，这份才是正本。

## 升级上游时

```bash
cd "D:\game mod\STS2MCP"
git stash                    # 工作区里的补丁先收起来（别用 checkout 扔掉）
git pull
git apply "D:\game mod\sts2core\patches\sts2mcp-statebuilder.patch"
powershell -File build.ps1
```

`git apply` 失败就手工合回去（`git stash show -p` 里也有一份），**四个都要合**。丢了不会报错：内核会悄悄退回去猜
（牌序重新洗、附魔按卡表算、战斗外看不到牌组）。合完重新导出一份 patch 覆盖旧的。

## 四个补丁各加了什么

| 补丁 | 加了什么 | 为什么值得打破「不改上游」这条规矩 |
|---|---|---|
| `draw_pile_order`（2026-08-29） | 抽牌堆的**真实牌序**，下标 0 是堆顶 | 原来的 `draw_pile` 被 mod 按稀有度 + id 重排过（为了和游戏内显示一致），真实顺序在那一步被扔掉了，而排序的输入本来就是真序。拿到真序之后，内核 `sync` 整堆照抄，一个随机数都不用掷 |
| `enchantment`（2026-09-01） | 每张**手牌**的附魔 `{id, name, amount}`（一张牌至多一个） | 在这之前附魔只体现在渲染好的 `description` 里：带「灵巧」的耸肩无视写着"获得 10 点格挡"，卡表说 8，JSON 里没有任何字段解释这个差。**照 id 模拟就会静默用错数值** |
| 牌堆的附魔（2026-09-03） | `draw_pile_order_enchantments`（和牌序逐位置配对的数组），外加弃牌堆 / 消耗堆每张牌的 `enchantment` | 上一条只补了手牌，于是**同一张牌在手里给 10、在牌堆里给 8**。`act1_f14_phantasmal_gardeners` 的整回合对拍就红在这里：那张耸肩无视是被剑柄打击**抽出来的**，内核看不见它的灵巧，算出 5+8=13，游戏给 15 |
| 牌组 `deck` / `deck_count`（2026-09-08） | **主牌组，每个界面都报**（形状和牌堆列表相同：名字含升级标记 + 费用 + 描述 + 附魔） | `BuildPlayerState` 里所有牌区都关在「战斗进行中」那个 `if` 里，于是**战斗外一张牌都看不到**，而拿牌 / 移除 / 升级 / 买牌**全都发生在战斗外**。`Player.Deck` 是主牌堆，在不在战斗里都存在 |

四个补丁都**只加字段**，不改任何已有输出。牌组那个补丁连
`record_trace.normalize()` 都不用改：它按白名单挑字段，多出来的键进不了 trace。

牌堆附魔必须挂在 `draw_pile_order` 上，不能挂在 `draw_pile` 上：后者被重排过，
下标不再指向同一张实体牌；而按名字回配，在"三张打击只有一张带附魔"时是欠定的。

## 还该打、没打的补丁

* **选牌屏的选中状态。** `BuildCardSelectState` 只给手牌选择屏构造 `selected_cards`，
  「从牌堆里挑几张」那类屏的选中状态看不到。和 `draw_pile_order` 是同一类纯读取补丁。
* **佩尔之翼的「献祭」按钮。** `ExecuteSkipCardReward` 写死了 `altButtons[0].ForceClick()`，
  只能点到「跳过」。要改的是 `McpMod.Actions.cs`。

这两处在实战里怎么绕，见 [driving.md](driving.md) 的「MCP 够不到的操作」。
