//! 进阶数值表 —— **生成产物，别手改**。
//!
//! 重新生成：
//! ```text
//! & "D:\game mod\sts2sim\.venv\Scripts\python.exe" tools/dump_ascension.py
//! ```
//!
//! 来源和口径写在 `tools/dump_ascension.py` 的文件头。三条要点：
//!
//! * **`[源码]` 档，验不了。** 全部实录都是 A1/A2，A8 以上没有任何观测。
//!   对应的安全性质是 [`adjust`] 在 `asc < 8` 时提前 return ——
//!   既有对拍逐字节不变，`ascension_below_8_is_a_no_op` 守着。
//! * **`def` / `mv` 是下标，`name` / `mv_name` 是它的校验位。**
//!   有人往 `ENEMIES` 中间插一条，下标就全歪了，而歪掉不会报错 ——
//!   `asc_table_indices_still_point_at_the_named_enemy` 守着。
//! * **对不上的没进表。** 生成器按 (种类, 低进阶值) 去认内核的 op，
//!   候选个数和 [源码] 的用点数对不上就整条跳过并报出来。
//!   欠的那些在 `data/ascension.json` 里逐条写着卡在哪。

use crate::ops::EOp;

/// 一只敌人的血量区间，两档。`low` 是 A8 以下，`high` 是 A8 及以上。
#[derive(Clone, Copy, Debug)]
pub struct AscHp {
    pub def: u16,
    pub name: &'static str,
    /// 到这一档才换值（8 = `ToughEnemies`）
    pub gate: u8,
    pub low: (i32, i32),
    pub high: (i32, i32),
}

/// 一个招式里的一个数值，两档。
#[derive(Clone, Copy, Debug)]
pub struct AscOp {
    pub def: u16,
    pub name: &'static str,
    pub mv: u8,
    pub mv_name: &'static str,
    /// `EnemyMove::ops` 里的下标
    pub op: u8,
    /// `true` = 改的是段数，`false` = 改的是数值
    pub hits: bool,
    /// 8 = `ToughEnemies` · 9 = `DeadlyEnemies`
    pub gate: u8,
    pub low: i32,
    pub high: i32,
    /// [源码] 里那个属性叫什么，出问题时回去查它
    pub prop: &'static str,
}

/// 进阶档位名（`AscensionLevel` 的枚举序号就是进阶数）。
pub static LEVEL_NAMES: &[(u8, &str)] = &[
    (1, "SwarmingElites"),
    (2, "WearyTraveler"),
    (3, "Poverty"),
    (4, "TightBelt"),
    (5, "AscendersBane"),
    (6, "Inflation"),
    (7, "Scarcity"),
    (8, "ToughEnemies"),
    (9, "DeadlyEnemies"),
    (10, "DoubleBoss"),
];

/// 血量两档，73 只。
pub static ASC_HP: &[AscHp] = &[
    AscHp { def: 4, name: "小啃兽", gate: 8, low: (42, 46), high: (44, 48) },  // Nibbit
    AscHp { def: 5, name: "旧日雕像", gate: 8, low: (127, 127), high: (132, 132) },  // BygoneEffigy
    AscHp { def: 6, name: "缩小甲虫", gate: 8, low: (38, 40), high: (40, 42) },  // ShrinkerBeetle
    AscHp { def: 7, name: "立柱构造体", gate: 8, low: (65, 65), high: (70, 70) },  // CubexConstruct
    AscHp { def: 8, name: "毛绒伏地虫", gate: 8, low: (55, 57), high: (58, 59) },  // FuzzyWurmCrawler
    AscHp { def: 9, name: "树叶史莱姆（小）", gate: 8, low: (11, 15), high: (12, 16) },  // LeafSlimeS
    AscHp { def: 10, name: "树叶史莱姆（中）", gate: 8, low: (32, 35), high: (33, 36) },  // LeafSlimeM
    AscHp { def: 11, name: "树枝史莱姆（小）", gate: 8, low: (7, 11), high: (8, 12) },  // TwigSlimeS
    AscHp { def: 12, name: "树枝史莱姆（中）", gate: 8, low: (26, 28), high: (27, 29) },  // TwigSlimeM
    AscHp { def: 13, name: "劫掠者弩手", gate: 8, low: (18, 21), high: (19, 22) },  // CrossbowRubyRaider
    AscHp { def: 14, name: "劫掠者斧手", gate: 8, low: (20, 22), high: (21, 23) },  // AxeRubyRaider
    AscHp { def: 15, name: "劫掠者追踪手", gate: 8, low: (21, 25), high: (22, 26) },  // TrackerRubyRaider
    AscHp { def: 16, name: "飞蝇菌子", gate: 8, low: (47, 49), high: (51, 53) },  // Flyconid
    AscHp { def: 17, name: "雾菇", gate: 8, low: (74, 74), high: (78, 78) },  // Fogmog
    AscHp { def: 19, name: "同族神官", gate: 8, low: (190, 190), high: (199, 199) },  // KinPriest
    AscHp { def: 20, name: "同族信徒", gate: 8, low: (58, 59), high: (62, 63) },  // KinFollower
    AscHp { def: 21, name: "盛碗虫（石）", gate: 8, low: (45, 48), high: (46, 49) },  // BowlbugRock
    AscHp { def: 22, name: "盛碗虫（卵）", gate: 8, low: (21, 22), high: (23, 24) },  // BowlbugEgg
    AscHp { def: 23, name: "偷窃草蜢", gate: 8, low: (79, 79), high: (84, 84) },  // ThievingHopper
    AscHp { def: 24, name: "盛碗虫（蜜）", gate: 8, low: (35, 38), high: (36, 39) },  // BowlbugNectar
    AscHp { def: 25, name: "盛碗虫（丝）", gate: 8, low: (40, 43), high: (41, 44) },  // BowlbugSilk
    AscHp { def: 26, name: "啃咬机", gate: 8, low: (60, 64), high: (63, 67) },  // Chomper
    AscHp { def: 27, name: "虱虫之祖", gate: 8, low: (134, 136), high: (138, 141) },  // LouseProgenitor
    AscHp { def: 28, name: "螨虫", gate: 8, low: (61, 67), high: (64, 69) },  // Myte
    AscHp { def: 29, name: "直飞产卵虫", gate: 8, low: (124, 130), high: (126, 132) },  // Ovicopter
    AscHp { def: 30, name: "棘蟾", gate: 8, low: (116, 119), high: (121, 124) },  // SpinyToad
    AscHp { def: 31, name: "蜂后", gate: 8, low: (400, 400), high: (419, 419) },  // Queen
    AscHp { def: 33, name: "知识恶魔", gate: 8, low: (379, 379), high: (399, 399) },  // KnowledgeDemon
    AscHp { def: 34, name: "碾碎爪", gate: 8, low: (209, 209), high: (219, 219) },  // Crusher
    AscHp { def: 35, name: "火箭", gate: 8, low: (199, 199), high: (209, 209) },  // Rocket
    AscHp { def: 36, name: "蛮兽", gate: 8, low: (72, 72), high: (76, 76) },  // Mawler
    AscHp { def: 37, name: "闪光贾克斯果", gate: 8, low: (31, 33), high: (34, 36) },  // SnappingJaxfruit
    AscHp { def: 38, name: "蛇行扼杀者", gate: 8, low: (53, 55), high: (54, 56) },  // SlitheringStrangler
    AscHp { def: 39, name: "扭动虫", gate: 8, low: (17, 21), high: (18, 22) },  // Wriggler
    AscHp { def: 40, name: "异蛙寄生虫", gate: 8, low: (61, 64), high: (66, 68) },  // PhrogParasite
    AscHp { def: 41, name: "仪式兽", gate: 8, low: (252, 252), high: (262, 262) },  // CeremonialBeast
    AscHp { def: 42, name: "外骨骼虫", gate: 8, low: (24, 28), high: (25, 29) },  // Exoskeleton
    AscHp { def: 43, name: "蜂群术士", gate: 8, low: (145, 145), high: (155, 155) },  // Entomancer
    AscHp { def: 44, name: "感染棱柱", gate: 8, low: (161, 161), high: (171, 171) },  // InfestedPrism
    AscHp { def: 45, name: "结实的卵", gate: 8, low: (14, 18), high: (15, 19) },  // ToughEgg
    AscHp { def: 47, name: "无厌沙虫", gate: 8, low: (321, 321), high: (341, 341) },  // TheInsatiable
    AscHp { def: 48, name: "虔诚雕刻师", gate: 8, low: (162, 162), high: (172, 172) },  // DevotedSculptor
    AscHp { def: 49, name: "永世沙漏", gate: 8, low: (512, 512), high: (535, 535) },  // Aeonglass
    AscHp { def: 50, name: "活体盾", gate: 8, low: (55, 55), high: (65, 65) },  // LivingShield
    AscHp { def: 51, name: "高塔炮手", gate: 8, low: (41, 41), high: (51, 51) },  // TurretOperator
    AscHp { def: 52, name: "猫头鹰法官", gate: 8, low: (231, 231), high: (247, 247) },  // OwlMagistrate
    AscHp { def: 53, name: "组装师", gate: 8, low: (150, 150), high: (155, 155) },  // Fabricator
    AscHp { def: 54, name: "噪音机器人", gate: 8, low: (18, 23), high: (19, 24) },  // Noisebot
    AscHp { def: 55, name: "电击机器人", gate: 8, low: (18, 23), high: (19, 24) },  // Zapbot
    AscHp { def: 58, name: "多尼斯异鸟", gate: 8, low: (81, 84), high: (90, 90) },  // Byrdonis
    AscHp { def: 59, name: "猎人杀手", gate: 8, low: (121, 121), high: (126, 126) },  // HunterKiller
    AscHp { def: 60, name: "残杀千足虫", gate: 8, low: (40, 46), high: (46, 52) },  // DecimillipedeSegmentBack
    AscHp { def: 60, name: "残杀千足虫", gate: 8, low: (40, 46), high: (46, 52) },  // DecimillipedeSegmentFront
    AscHp { def: 60, name: "残杀千足虫", gate: 8, low: (40, 46), high: (46, 52) },  // DecimillipedeSegmentMiddle
    AscHp { def: 61, name: "青蛙骑士", gate: 8, low: (191, 191), high: (199, 199) },  // FrogKnight
    AscHp { def: 62, name: "巨斧机器人", gate: 8, low: (70, 78), high: (76, 86) },  // Axebot
    AscHp { def: 63, name: "灵魂枢纽", gate: 8, low: (234, 234), high: (254, 254) },  // SoulNexus
    AscHp { def: 65, name: "骇鳗", gate: 8, low: (140, 140), high: (150, 150) },  // TerrorEel
    AscHp { def: 66, name: "活雾", gate: 8, low: (80, 80), high: (82, 82) },  // LivingFog
    AscHp { def: 67, name: "气态炸弹", gate: 8, low: (7, 7), high: (8, 8) },  // GasBomb
    AscHp { def: 68, name: "花园幽灵鳗", gate: 8, low: (26, 31), high: (27, 32) },  // PhantasmalGardener
    AscHp { def: 69, name: "瀑布巨兽", gate: 8, low: (240, 240), high: (250, 250) },  // WaterfallGiant
    AscHp { def: 70, name: "地道虫", gate: 8, low: (87, 87), high: (92, 92) },  // Tunneler
    AscHp { def: 71, name: "胧光怪", gate: 8, low: (123, 123), high: (129, 129) },  // TheObscura
    AscHp { def: 73, name: "淤泥旋螺", gate: 8, low: (37, 39), high: (41, 42) },  // SludgeSpinner
    AscHp { def: 74, name: "噬尸蛞蝓", gate: 8, low: (25, 27), high: (27, 29) },  // CorpseSlug
    AscHp { def: 75, name: "海洋混混", gate: 8, low: (44, 46), high: (47, 49) },  // Seapunk
    AscHp { def: 76, name: "钙化邪教徒", gate: 8, low: (38, 41), high: (39, 42) },  // CalcifiedCultist
    AscHp { def: 77, name: "潮湿邪教徒", gate: 8, low: (51, 53), high: (52, 54) },  // DampCultist
    AscHp { def: 78, name: "下水道蚌", gate: 8, low: (56, 56), high: (58, 58) },  // SewerClam
    AscHp { def: 79, name: "双尾鼠", gate: 8, low: (17, 21), high: (18, 22) },  // TwoTailedRat
    AscHp { def: 80, name: "拳击构装体", gate: 8, low: (55, 55), high: (60, 60) },  // PunchConstruct
    AscHp { def: 81, name: "灵魂异鱼", gate: 8, low: (211, 211), high: (221, 221) },  // SoulFysh
];

/// 招式数值两档，160 条。
pub static ASC_OPS: &[AscOp] = &[
    AscOp { def: 4, name: "小啃兽", mv: 0, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 12, high: 13, prop: "ButtDamage" },
    AscOp { def: 4, name: "小啃兽", mv: 1, mv_name: "啃咬并戒备", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "SliceDamage" },
    AscOp { def: 4, name: "小啃兽", mv: 1, mv_name: "啃咬并戒备", op: 1, hits: false, gate: 8, low: 5, high: 6, prop: "SliceBlock" },
    AscOp { def: 4, name: "小啃兽", mv: 2, mv_name: "嘶鸣", op: 0, hits: false, gate: 9, low: 2, high: 3, prop: "HissStrengthGain" },
    AscOp { def: 5, name: "旧日雕像", mv: 2, mv_name: "挥砍", op: 0, hits: false, gate: 9, low: 13, high: 15, prop: "SlashDamage" },
    AscOp { def: 6, name: "缩小甲虫", mv: 1, mv_name: "啃咬", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "ChompDamage" },
    AscOp { def: 6, name: "缩小甲虫", mv: 2, mv_name: "踩踏", op: 0, hits: false, gate: 9, low: 13, high: 14, prop: "StompDamage" },
    AscOp { def: 7, name: "立柱构造体", mv: 1, mv_name: "连发", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "BlastDamage" },
    AscOp { def: 7, name: "立柱构造体", mv: 2, mv_name: "连发", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "BlastDamage" },
    AscOp { def: 7, name: "立柱构造体", mv: 3, mv_name: "排射", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "ExpelDamage" },
    AscOp { def: 9, name: "树叶史莱姆（小）", mv: 0, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "TackleDamage" },
    AscOp { def: 10, name: "树叶史莱姆（中）", mv: 1, mv_name: "团射", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ClumpDamage" },
    AscOp { def: 11, name: "树枝史莱姆（小）", mv: 0, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 4, high: 5, prop: "TackleDamage" },
    AscOp { def: 12, name: "树枝史莱姆（中）", mv: 1, mv_name: "团射", op: 0, hits: false, gate: 9, low: 11, high: 12, prop: "ClumpDamage" },
    AscOp { def: 13, name: "劫掠者弩手", mv: 1, mv_name: "射击", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "FireDamage" },
    AscOp { def: 14, name: "劫掠者斧手", mv: 0, mv_name: "劈砍并戒备", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "SwingDamage" },
    AscOp { def: 14, name: "劫掠者斧手", mv: 0, mv_name: "劈砍并戒备", op: 1, hits: false, gate: 9, low: 5, high: 6, prop: "SwingBlock" },
    AscOp { def: 14, name: "劫掠者斧手", mv: 1, mv_name: "劈砍并戒备", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "SwingDamage" },
    AscOp { def: 14, name: "劫掠者斧手", mv: 1, mv_name: "劈砍并戒备", op: 1, hits: false, gate: 9, low: 5, high: 6, prop: "SwingBlock" },
    AscOp { def: 14, name: "劫掠者斧手", mv: 2, mv_name: "重劈", op: 0, hits: false, gate: 9, low: 12, high: 13, prop: "BigSwingDamage" },
    AscOp { def: 15, name: "劫掠者追踪手", mv: 1, mv_name: "放狗", op: 0, hits: false, gate: 9, low: 1, high: 1, prop: "HoundsDamage" },
    AscOp { def: 15, name: "劫掠者追踪手", mv: 1, mv_name: "放狗", op: 0, hits: true, gate: 9, low: 8, high: 9, prop: "HoundsRepeat" },
    AscOp { def: 16, name: "飞蝇菌子", mv: 0, mv_name: "脆弱孢子", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "SporeDamage" },
    AscOp { def: 16, name: "飞蝇菌子", mv: 1, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 11, high: 12, prop: "SmashDamage" },
    AscOp { def: 17, name: "雾菇", mv: 1, mv_name: "挥爪", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "SwipeDamage" },
    AscOp { def: 17, name: "雾菇", mv: 2, mv_name: "头槌", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "HeadbuttDamage" },
    AscOp { def: 17, name: "雾菇", mv: 3, mv_name: "挥爪", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "SwipeDamage" },
    AscOp { def: 19, name: "同族神官", mv: 2, mv_name: "光束", op: 0, hits: false, gate: 9, low: 3, high: 3, prop: "BeamDamage" },
    AscOp { def: 19, name: "同族神官", mv: 3, mv_name: "仪式", op: 0, hits: false, gate: 9, low: 2, high: 3, prop: "RitualStrength" },
    AscOp { def: 20, name: "同族信徒", mv: 0, mv_name: "快斩", op: 0, hits: false, gate: 9, low: 5, high: 5, prop: "QuickSlashDamage" },
    AscOp { def: 20, name: "同族信徒", mv: 1, mv_name: "回旋镖", op: 0, hits: false, gate: 9, low: 2, high: 2, prop: "BoomerangDamage" },
    AscOp { def: 20, name: "同族信徒", mv: 2, mv_name: "战舞", op: 0, hits: false, gate: 9, low: 2, high: 3, prop: "DanceStrength" },
    AscOp { def: 21, name: "盛碗虫（石）", mv: 0, mv_name: "头槌", op: 0, hits: false, gate: 9, low: 15, high: 16, prop: "HeadbuttDamage" },
    AscOp { def: 22, name: "盛碗虫（卵）", mv: 0, mv_name: "撕咬并戒备", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "BiteDamage" },
    AscOp { def: 22, name: "盛碗虫（卵）", mv: 0, mv_name: "撕咬并戒备", op: 1, hits: false, gate: 9, low: 7, high: 8, prop: "ProtectBlock" },
    AscOp { def: 23, name: "偷窃草蜢", mv: 0, mv_name: "偷盗", op: 0, hits: false, gate: 9, low: 17, high: 19, prop: "TheftDamage" },
    AscOp { def: 23, name: "偷窃草蜢", mv: 2, mv_name: "帽子戏法", op: 0, hits: false, gate: 9, low: 21, high: 23, prop: "HatTrickDamage" },
    AscOp { def: 23, name: "偷窃草蜢", mv: 3, mv_name: "抓取", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "NabDamage" },
    AscOp { def: 24, name: "盛碗虫（蜜）", mv: 1, mv_name: "强化", op: 0, hits: false, gate: 9, low: 15, high: 16, prop: "BuffStrengthGain" },
    AscOp { def: 26, name: "啃咬机", mv: 0, mv_name: "钳夹", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ClampDamage" },
    AscOp { def: 27, name: "虱虫之祖", mv: 0, mv_name: "蛛网大炮", op: 0, hits: false, gate: 9, low: 9, high: 10, prop: "WebDamage" },
    AscOp { def: 27, name: "虱虫之祖", mv: 1, mv_name: "蜷缩生长", op: 0, hits: false, gate: 8, low: 14, high: 18, prop: "CurlBlock" },
    AscOp { def: 27, name: "虱虫之祖", mv: 2, mv_name: "扑击", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "PounceDamage" },
    AscOp { def: 28, name: "螨虫", mv: 1, mv_name: "撕咬", op: 0, hits: false, gate: 9, low: 13, high: 15, prop: "BiteDamage" },
    AscOp { def: 28, name: "螨虫", mv: 2, mv_name: "吸血", op: 0, hits: false, gate: 9, low: 4, high: 6, prop: "SuckDamage" },
    AscOp { def: 29, name: "直飞产卵虫", mv: 1, mv_name: "摧毁", op: 0, hits: false, gate: 9, low: 16, high: 17, prop: "SmashDamage" },
    AscOp { def: 29, name: "直飞产卵虫", mv: 2, mv_name: "嫩化", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "TenderizerDamage" },
    AscOp { def: 29, name: "直飞产卵虫", mv: 3, mv_name: "营养糊", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "NutritionalPasteStrengthAmount" },
    AscOp { def: 30, name: "棘蟾", mv: 1, mv_name: "尖刺爆炸", op: 0, hits: false, gate: 9, low: 23, high: 25, prop: "ExplosionDamage" },
    AscOp { def: 30, name: "棘蟾", mv: 2, mv_name: "舌刺", op: 0, hits: false, gate: 9, low: 17, high: 19, prop: "LashDamage" },
    AscOp { def: 31, name: "蜂后", mv: 1, mv_name: "为我燃烧", op: 1, hits: false, gate: 9, low: 1, high: 1, prop: "strengthAmount" },
    AscOp { def: 31, name: "蜂后", mv: 2, mv_name: "斩首", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "OffWithYourHeadDamage" },
    AscOp { def: 31, name: "蜂后", mv: 3, mv_name: "处决", op: 0, hits: false, gate: 9, low: 15, high: 18, prop: "ExecutionDamage" },
    AscOp { def: 33, name: "知识恶魔", mv: 1, mv_name: "拍击", op: 0, hits: false, gate: 9, low: 17, high: 18, prop: "SlapDamage" },
    AscOp { def: 33, name: "知识恶魔", mv: 2, mv_name: "知识淹没", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "KnowledgeOverwhelmingDamage" },
    AscOp { def: 33, name: "知识恶魔", mv: 3, mv_name: "深思", op: 0, hits: false, gate: 9, low: 11, high: 13, prop: "PonderDamage" },
    AscOp { def: 34, name: "碾碎爪", mv: 1, mv_name: "增幅打击", op: 0, hits: false, gate: 9, low: 4, high: 4, prop: "EnlargingStrikeDamage" },
    AscOp { def: 34, name: "碾碎爪", mv: 2, mv_name: "虫刺", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "BugStingDamage" },
    AscOp { def: 35, name: "火箭", mv: 0, mv_name: "瞄准镜", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "TargetingReticleDamage" },
    AscOp { def: 35, name: "火箭", mv: 1, mv_name: "精准光束", op: 0, hits: false, gate: 9, low: 18, high: 20, prop: "PrecisionBeamDamage" },
    AscOp { def: 35, name: "火箭", mv: 2, mv_name: "充能", op: 0, hits: false, gate: 9, low: 2, high: 3, prop: "ChargeUpStrengthGain" },
    AscOp { def: 35, name: "火箭", mv: 3, mv_name: "激光", op: 0, hits: false, gate: 9, low: 31, high: 35, prop: "LaserDamage" },
    AscOp { def: 36, name: "蛮兽", mv: 0, mv_name: "撕裂", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "RipAndTearDamage" },
    AscOp { def: 36, name: "蛮兽", mv: 2, mv_name: "爪击", op: 0, hits: false, gate: 9, low: 4, high: 5, prop: "ClawDamage" },
    AscOp { def: 37, name: "闪光贾克斯果", mv: 0, mv_name: "能量球", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "EnergyDamage" },
    AscOp { def: 38, name: "蛇行扼杀者", mv: 1, mv_name: "重击", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "ThwackDamage" },
    AscOp { def: 38, name: "蛇行扼杀者", mv: 2, mv_name: "鞭击", op: 0, hits: false, gate: 9, low: 12, high: 13, prop: "LashDamage" },
    AscOp { def: 39, name: "扭动虫", mv: 1, mv_name: "啃咬", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "BiteDamage" },
    AscOp { def: 40, name: "异蛙寄生虫", mv: 1, mv_name: "鞭击", op: 0, hits: false, gate: 9, low: 4, high: 5, prop: "LashDamage" },
    AscOp { def: 41, name: "仪式兽", mv: 0, mv_name: "耕地", op: 0, hits: false, gate: 9, low: 18, high: 20, prop: "PlowDamage" },
    AscOp { def: 41, name: "仪式兽", mv: 1, mv_name: "践地", op: 0, hits: false, gate: 9, low: 150, high: 160, prop: "PlowAmount" },
    AscOp { def: 41, name: "仪式兽", mv: 4, mv_name: "践踏", op: 0, hits: false, gate: 9, low: 15, high: 17, prop: "StompDamage" },
    AscOp { def: 41, name: "仪式兽", mv: 5, mv_name: "碾碎", op: 0, hits: false, gate: 9, low: 17, high: 19, prop: "CrushDamage" },
    AscOp { def: 41, name: "仪式兽", mv: 5, mv_name: "碾碎", op: 1, hits: false, gate: 9, low: 3, high: 4, prop: "CrushStrength" },
    AscOp { def: 42, name: "外骨骼虫", mv: 0, mv_name: "疾走", op: 0, hits: true, gate: 9, low: 3, high: 4, prop: "SkitterRepeats" },
    AscOp { def: 42, name: "外骨骼虫", mv: 1, mv_name: "大颚", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "MandiblesDamage" },
    AscOp { def: 43, name: "蜂群术士", mv: 0, mv_name: "蜂群", op: 0, hits: true, gate: 9, low: 7, high: 8, prop: "BeesRepeat" },
    AscOp { def: 43, name: "蜂群术士", mv: 0, mv_name: "蜂群", op: 0, hits: false, gate: 9, low: 3, high: 3, prop: "BeesDamage" },
    AscOp { def: 43, name: "蜂群术士", mv: 1, mv_name: "长矛", op: 0, hits: false, gate: 9, low: 18, high: 20, prop: "SpearMoveDamage" },
    AscOp { def: 44, name: "感染棱柱", mv: 0, mv_name: "戳刺", op: 0, hits: false, gate: 9, low: 15, high: 17, prop: "JabDamage" },
    AscOp { def: 44, name: "感染棱柱", mv: 1, mv_name: "辐射", op: 0, hits: false, gate: 9, low: 11, high: 13, prop: "RadiateDamage" },
    AscOp { def: 44, name: "感染棱柱", mv: 1, mv_name: "辐射", op: 1, hits: false, gate: 9, low: 11, high: 13, prop: "RadiateBlock" },
    AscOp { def: 44, name: "感染棱柱", mv: 2, mv_name: "旋风", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "WhirlwindDamage" },
    AscOp { def: 44, name: "感染棱柱", mv: 3, mv_name: "脉动", op: 0, hits: false, gate: 9, low: 8, high: 10, prop: "PulsateDamage" },
    AscOp { def: 44, name: "感染棱柱", mv: 3, mv_name: "脉动", op: 1, hits: false, gate: 9, low: 2, high: 3, prop: "VitalSparkAmount" },
    AscOp { def: 44, name: "感染棱柱", mv: 3, mv_name: "脉动", op: 2, hits: false, gate: 8, low: 20, high: 22, prop: "PulsateBlock" },
    AscOp { def: 45, name: "结实的卵", mv: 1, mv_name: "啃咬", op: 0, hits: false, gate: 9, low: 4, high: 5, prop: "NibbleDamage" },
    AscOp { def: 47, name: "无厌沙虫", mv: 1, mv_name: "鞭挞", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ThrashDamage" },
    AscOp { def: 47, name: "无厌沙虫", mv: 2, mv_name: "猛扑撕咬", op: 0, hits: false, gate: 9, low: 28, high: 31, prop: "BiteDamage" },
    AscOp { def: 47, name: "无厌沙虫", mv: 3, mv_name: "垂涎", op: 0, hits: false, gate: 9, low: 2, high: 3, prop: "SalivateStrength" },
    AscOp { def: 47, name: "无厌沙虫", mv: 4, mv_name: "鞭挞2", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ThrashDamage" },
    AscOp { def: 48, name: "虔诚雕刻师", mv: 1, mv_name: "狂暴", op: 0, hits: false, gate: 9, low: 12, high: 15, prop: "SavageDamage" },
    AscOp { def: 49, name: "永世沙漏", mv: 0, mv_name: "退潮", op: 0, hits: false, gate: 9, low: 26, high: 32, prop: "EbbDamage" },
    AscOp { def: 49, name: "永世沙漏", mv: 1, mv_name: "眼部激光", op: 0, hits: false, gate: 9, low: 11, high: 12, prop: "EyeLasersDamage" },
    AscOp { def: 49, name: "永世沙漏", mv: 2, mv_name: "剧烈增强", op: 0, hits: false, gate: 9, low: 1, high: 2, prop: "WitherAmount" },
    AscOp { def: 50, name: "活体盾", mv: 1, mv_name: "猛砸", op: 0, hits: false, gate: 9, low: 16, high: 18, prop: "SmashDamage" },
    AscOp { def: 51, name: "高塔炮手", mv: 0, mv_name: "倾泻火力", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "FireDamage" },
    AscOp { def: 51, name: "高塔炮手", mv: 1, mv_name: "倾泻火力2", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "FireDamage" },
    AscOp { def: 52, name: "猫头鹰法官", mv: 0, mv_name: "审视", op: 0, hits: false, gate: 9, low: 16, high: 17, prop: "ScrutinyDamage" },
    AscOp { def: 52, name: "猫头鹰法官", mv: 1, mv_name: "啄击突袭", op: 0, hits: false, gate: 9, low: 4, high: 4, prop: "PeckAssaultDamage" },
    AscOp { def: 52, name: "猫头鹰法官", mv: 3, mv_name: "判决", op: 0, hits: false, gate: 9, low: 33, high: 36, prop: "VerdictDamage" },
    AscOp { def: 53, name: "组装师", mv: 1, mv_name: "制造打击", op: 0, hits: false, gate: 9, low: 18, high: 21, prop: "FabricatingStrikeDamage" },
    AscOp { def: 53, name: "组装师", mv: 2, mv_name: "分解", op: 0, hits: false, gate: 9, low: 11, high: 13, prop: "DisintegrateDamage" },
    AscOp { def: 55, name: "电击机器人", mv: 0, mv_name: "电击", op: 0, hits: false, gate: 9, low: 14, high: 15, prop: "ZapDamage" },
    AscOp { def: 58, name: "多尼斯异鸟", mv: 0, mv_name: "俯冲", op: 0, hits: false, gate: 9, low: 17, high: 19, prop: "SwoopDamage" },
    AscOp { def: 58, name: "多尼斯异鸟", mv: 1, mv_name: "啄击", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "PeckDamage" },
    AscOp { def: 58, name: "多尼斯异鸟", mv: 1, mv_name: "啄击", op: 0, hits: true, gate: 9, low: 3, high: 3, prop: "PeckRepeat" },
    AscOp { def: 59, name: "猎人杀手", mv: 1, mv_name: "撕咬", op: 0, hits: false, gate: 9, low: 17, high: 19, prop: "BiteDamage" },
    AscOp { def: 59, name: "猎人杀手", mv: 2, mv_name: "穿刺", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "PunctureDamage" },
    AscOp { def: 61, name: "青蛙骑士", mv: 0, mv_name: "舌鞭", op: 0, hits: false, gate: 9, low: 13, high: 14, prop: "TongueLashDamage" },
    AscOp { def: 61, name: "青蛙骑士", mv: 1, mv_name: "除恶", op: 0, hits: false, gate: 9, low: 21, high: 23, prop: "StrikeDownEvilDamage" },
    AscOp { def: 61, name: "青蛙骑士", mv: 3, mv_name: "甲虫冲锋", op: 0, hits: false, gate: 9, low: 35, high: 40, prop: "BeetleChargeDamage" },
    AscOp { def: 62, name: "巨斧机器人", mv: 0, mv_name: "启动", op: 0, hits: false, gate: 9, low: 10, high: 15, prop: "BootUpBlock" },
    AscOp { def: 62, name: "巨斧机器人", mv: 1, mv_name: "锤击上勾拳", op: 0, hits: false, gate: 9, low: 12, high: 14, prop: "HammerUppercutDamage" },
    AscOp { def: 62, name: "巨斧机器人", mv: 2, mv_name: "一二连击", op: 0, hits: false, gate: 9, low: 9, high: 10, prop: "OneTwoDamage" },
    AscOp { def: 63, name: "灵魂枢纽", mv: 0, mv_name: "灵魂灼烧", op: 0, hits: false, gate: 9, low: 29, high: 31, prop: "SoulBurnDamage" },
    AscOp { def: 63, name: "灵魂枢纽", mv: 1, mv_name: "大漩涡", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "MaelstromDamage" },
    AscOp { def: 63, name: "灵魂枢纽", mv: 1, mv_name: "大漩涡", op: 0, hits: true, gate: 9, low: 4, high: 4, prop: "MaelstromRepeat" },
    AscOp { def: 63, name: "灵魂枢纽", mv: 2, mv_name: "汲取生命", op: 0, hits: false, gate: 9, low: 18, high: 19, prop: "DrainLifeDamage" },
    AscOp { def: 64, name: "实验体", mv: 1, mv_name: "啃咬", op: 0, hits: false, gate: 9, low: 20, high: 22, prop: "BiteDamage" },
    AscOp { def: 64, name: "实验体", mv: 2, mv_name: "头槌猛击", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "SkullBashDamage" },
    AscOp { def: 64, name: "实验体", mv: 6, mv_name: "灼热咆哮", op: 0, hits: false, gate: 9, low: 3, high: 5, prop: "BurningGrowlBurnCount" },
    AscOp { def: 64, name: "实验体", mv: 6, mv_name: "灼热咆哮", op: 1, hits: false, gate: 9, low: 2, high: 3, prop: "EnrageAmount" },
    AscOp { def: 64, name: "实验体", mv: 6, mv_name: "灼热咆哮", op: 1, hits: false, gate: 9, low: 2, high: 3, prop: "BurningGrowlStrengthGain" },
    AscOp { def: 65, name: "骇鳗", mv: 0, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 16, high: 18, prop: "CrashDamage" },
    AscOp { def: 65, name: "骇鳗", mv: 1, mv_name: "乱舞", op: 0, hits: false, gate: 9, low: 3, high: 4, prop: "ThrashDamage" },
    AscOp { def: 66, name: "活雾", mv: 1, mv_name: "膨胀", op: 1, hits: false, gate: 9, low: 5, high: 6, prop: "BloatDamage" },
    AscOp { def: 67, name: "气态炸弹", mv: 0, mv_name: "自爆", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ExplodeDamage" },
    AscOp { def: 68, name: "花园幽灵鳗", mv: 0, mv_name: "啃咬", op: 0, hits: false, gate: 9, low: 5, high: 5, prop: "BiteDamage" },
    AscOp { def: 68, name: "花园幽灵鳗", mv: 1, mv_name: "鞭击", op: 0, hits: false, gate: 9, low: 7, high: 7, prop: "LashDamage" },
    AscOp { def: 68, name: "花园幽灵鳗", mv: 2, mv_name: "乱舞", op: 0, hits: true, gate: 9, low: 3, high: 3, prop: "FlailRepeat" },
    AscOp { def: 69, name: "瀑布巨兽", mv: 0, mv_name: "加压", op: 0, hits: false, gate: 9, low: 15, high: 20, prop: "PressurizeAmount" },
    AscOp { def: 69, name: "瀑布巨兽", mv: 1, mv_name: "重踏", op: 0, hits: false, gate: 9, low: 15, high: 16, prop: "StompDamage" },
    AscOp { def: 69, name: "瀑布巨兽", mv: 2, mv_name: "撞击", op: 0, hits: false, gate: 9, low: 10, high: 11, prop: "RamDamage" },
    AscOp { def: 69, name: "瀑布巨兽", mv: 5, mv_name: "升压", op: 0, hits: false, gate: 9, low: 13, high: 14, prop: "PressureUpDamage" },
    AscOp { def: 70, name: "地道虫", mv: 0, mv_name: "咬击", op: 0, hits: false, gate: 9, low: 13, high: 15, prop: "BiteDamage" },
    AscOp { def: 70, name: "地道虫", mv: 1, mv_name: "钻地", op: 0, hits: false, gate: 8, low: 32, high: 37, prop: "BlockGain" },
    AscOp { def: 70, name: "地道虫", mv: 2, mv_name: "地底突袭", op: 0, hits: false, gate: 9, low: 23, high: 26, prop: "BelowDamage" },
    AscOp { def: 71, name: "胧光怪", mv: 1, mv_name: "穿刺凝视", op: 0, hits: false, gate: 9, low: 10, high: 11, prop: "PiercingGazeDamage" },
    AscOp { def: 71, name: "胧光怪", mv: 3, mv_name: "硬化打击", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "HardeningStrikeDamage" },
    AscOp { def: 71, name: "胧光怪", mv: 3, mv_name: "硬化打击", op: 1, hits: false, gate: 9, low: 6, high: 7, prop: "HardeningStrikeBlock" },
    AscOp { def: 72, name: "寄生惧魔", mv: 1, mv_name: "猛撞", op: 0, hits: false, gate: 9, low: 16, high: 17, prop: "SlamDamage" },
    AscOp { def: 73, name: "淤泥旋螺", mv: 0, mv_name: "喷油", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "OilSprayDamage" },
    AscOp { def: 73, name: "淤泥旋螺", mv: 1, mv_name: "猛砸", op: 0, hits: false, gate: 9, low: 11, high: 12, prop: "SlamDamage" },
    AscOp { def: 73, name: "淤泥旋螺", mv: 2, mv_name: "狂怒", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "RageDamage" },
    AscOp { def: 74, name: "噬尸蛞蝓", mv: 1, mv_name: "吞噬", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "GlompDamage" },
    AscOp { def: 75, name: "海洋混混", mv: 0, mv_name: "海踢", op: 0, hits: false, gate: 9, low: 11, high: 13, prop: "SeaKickDamage" },
    AscOp { def: 75, name: "海洋混混", mv: 2, mv_name: "泡泡嗝", op: 0, hits: false, gate: 8, low: 7, high: 8, prop: "BubbleBlock" },
    AscOp { def: 75, name: "海洋混混", mv: 2, mv_name: "泡泡嗝", op: 1, hits: false, gate: 9, low: 1, high: 2, prop: "BubbleStr" },
    AscOp { def: 76, name: "钙化邪教徒", mv: 1, mv_name: "黑暗打击", op: 0, hits: false, gate: 9, low: 9, high: 11, prop: "DarkStrikeDamage" },
    AscOp { def: 77, name: "潮湿邪教徒", mv: 0, mv_name: "吟唱", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "IncantationAmount" },
    AscOp { def: 77, name: "潮湿邪教徒", mv: 1, mv_name: "黑暗打击", op: 0, hits: false, gate: 9, low: 1, high: 3, prop: "DarkStrikeDamage" },
    AscOp { def: 78, name: "下水道蚌", mv: 0, mv_name: "喷射", op: 0, hits: false, gate: 9, low: 10, high: 11, prop: "JetDamage" },
    AscOp { def: 79, name: "双尾鼠", mv: 0, mv_name: "抓挠", op: 0, hits: false, gate: 9, low: 8, high: 9, prop: "ScratchDamage" },
    AscOp { def: 79, name: "双尾鼠", mv: 1, mv_name: "病咬", op: 0, hits: false, gate: 9, low: 6, high: 7, prop: "DiseaseBiteDamage" },
    AscOp { def: 80, name: "拳击构装体", mv: 1, mv_name: "快拳", op: 0, hits: false, gate: 9, low: 5, high: 6, prop: "FastPunchDamage" },
    AscOp { def: 80, name: "拳击构装体", mv: 2, mv_name: "重拳", op: 0, hits: false, gate: 9, low: 14, high: 16, prop: "StrongPunchDamage" },
    AscOp { def: 81, name: "灵魂异鱼", mv: 1, mv_name: "排气", op: 0, hits: false, gate: 9, low: 16, high: 17, prop: "DeGasDamage" },
    AscOp { def: 81, name: "灵魂异鱼", mv: 2, mv_name: "凝视", op: 0, hits: false, gate: 9, low: 7, high: 8, prop: "GazeDamage" },
    AscOp { def: 81, name: "灵魂异鱼", mv: 4, mv_name: "尖啸", op: 0, hits: false, gate: 9, low: 13, high: 15, prop: "ScreamDamage" },
];

/// 这一招的这个 op，在进阶 `asc` 下是什么样。
///
/// **这是内核里唯一一处把进阶算进 `EOp` 的地方。** 读 `EnemyMove::ops` 的
/// 八个地方（执行、两个威胁预测、planner、`move_signature`、两个验收台）
/// 全部经由它 —— 漏掉一个的表现是那条路径**静默地**按低进阶算。
///
/// `asc < 8` 时逐字返回原 op，一次表都不查。
#[inline]
pub fn adjust(def: u16, mv: usize, op_ix: usize, asc: u8, op: EOp) -> EOp {
    if asc < 8 {
        return op;
    }
    for r in ASC_OPS {
        if r.def == def && r.mv as usize == mv && r.op as usize == op_ix && asc >= r.gate {
            return patch(op, r.high, r.hits);
        }
    }
    op
}

/// 把新值写回 `EOp` 的对应字段。**认不出的 op 原样返回** ——
/// 生成器只会为它认识的那几种 op 建行，这里是兜底。
fn patch(op: EOp, v: i32, hits_field: bool) -> EOp {
    match op {
        EOp::Attack { base, hits } => {
            if hits_field {
                EOp::Attack { base, hits: v }
            } else {
                EOp::Attack { base: v, hits }
            }
        }
        EOp::AttackPlusStackHits { base, hits, per } => {
            if hits_field {
                EOp::AttackPlusStackHits { base, hits: v, per }
            } else {
                EOp::AttackPlusStackHits { base: v, hits, per }
            }
        }
        EOp::Block(_) => EOp::Block(v),
        EOp::SelfStatus { st, .. } => EOp::SelfStatus { st, amt: v },
        EOp::PlayerStatus { st, .. } => EOp::PlayerStatus { st, amt: v },
        EOp::AddCardToDiscard { card, .. } => EOp::AddCardToDiscard { card, count: v },
        other => other,
    }
}

/// 这只敌人在进阶 `asc` 下的血量区间。表里没有就返回 `None`
/// —— 调用方该退回 `EnemyDef::max_hp`（那是 A1/A2 实测的点值）。
pub fn hp_range(def: u16, asc: u8) -> Option<(i32, i32)> {
    for r in ASC_HP {
        if r.def == def {
            return Some(if asc >= r.gate { r.high } else { r.low });
        }
    }
    None
}
