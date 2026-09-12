//! **遭遇表** —— 「这一幕会遇到什么」，从 `data/` 读进来。
//!
//! 两份 JSON，两个档次，`tools/dump_encounters.py` 生成：
//!
//! | 文件 | 是什么 | 档次 |
//! |---|---|---|
//! | `data/encounters.json` | 幕 → 遭遇 → 怪物（英文类名）+ 房间构成 | **[源码]** |
//! | `data/enemy_ids.json` | 英文类名 ↔ 内核敌人的中文名 | **[实录]**（连接键只有实录）|
//!
//! # 这里一条解析规则都没有
//!
//! 两份 JSON 已经是解析好的表，本模块只做**查表和连接**：
//! 类名 -> `enemy_ids.json` 的 `kernel_enemy` -> [`crate::replay::enemy_id`]。
//! 最后那一步走 `replay` 那份名字解析（别名表 + `#` 后缀），
//! **不另抄一份** —— `dump_encounters.py` 也是读它，三处口径因此是同一个。
//!
//! # 它敢于放弃
//!
//! `GenerateMonsters()` 里带随机的那些遭遇（史莱姆掷 `NextBool`、
//! 劫掠者抽 3 次）在表里是 `exact: false`，只有一份 `all_possible`。
//! [`Table::resolve`] 对它们**整条拒绝**，返回 [`Unresolved::Inexact`] ——
//! 编一个自洽的多重集出来是这个仓库明令不做的事。
//! 认得出来的那些由 `data/encounters_overrides.json` 手填（那份也在这里读）。

use std::collections::BTreeMap;

use crate::json::{parse, Json};
use crate::synth::EnemySpec;

/// 一场遭遇。
#[derive(Clone, Debug)]
pub struct Encounter {
    pub key: String,
    /// `Monster` / `Elite` / `Boss`
    pub room_type: String,
    pub is_weak: bool,
    /// 构成确定时的怪物类名（重复出现就是几只）。`exact == false` 时是 `None`
    pub monsters: Option<Vec<String>>,
    /// 可能出现的全部怪物类名（构成随机时只有这一份）。
    ///
    /// **含 `choice_fields` 里的那些。** `dump_encounters.py` 把
    /// `_workerValidCounts` 那种"从这几个里抽"的字段单独记了一栏，
    /// 而认一场仗的时候它和 `all_possible` 是同一件事 ——
    /// 不并进来的话，盛碗虫那几场（石 + 卵/蜜/丝）一场都认不出来。
    pub all_possible: Vec<String>,
    pub exact: bool,
    /// 解析器为什么放弃（`exact == false` 时）
    pub note: Option<String>,
    /// [源码] `EncounterModel.Tags`。**抽序列时是个约束**：
    /// `ActModel.AddWithoutRepeatingTags` 不让相邻两场共享 tag
    /// （连着两场史莱姆），见 [`crate::synth::act`]。没有 tag 的一律空表。
    pub tags: Vec<String>,
}

/// 一幕。
#[derive(Clone, Debug)]
pub struct Act {
    pub name: String,
    pub index: i64,
    pub encounters: Vec<String>,
    /// [源码] `ActModel.NumberOfWeakEncounters` —— 开头几场从**弱怪**池里抽
    pub weak_encounters: usize,
    /// [源码] `ActModel.BaseNumberOfRooms` —— 这一幕预抽几场普通遭遇
    /// （**不含 Boss 和上古**）。这是「预抽多少」不是「走几间」——
    /// 走几间是路线，[`crate::synth::act`] 由调用方给
    pub base_rooms: usize,
}

/// 解析不出遭遇时的原因。**每一条都指向一个具体的补法**。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unresolved {
    /// 表里没有这个 key
    NoSuchEncounter(String),
    /// 构成含随机，`dump_encounters.py` 标了 `exact: false`。
    /// 补法：`data/encounters_overrides.json` 手填
    Inexact { key: String, note: String },
    /// 这只怪物还没有「英文类名 ↔ 内核敌人」的连接键（没在实录里见过）
    NoKernelName { key: String, monster: String },
    /// 有连接键，但内核的 `ENEMIES` 里没有这只
    NotInKernel { key: String, monster: String, kernel_name: String },
}

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unresolved::NoSuchEncounter(k) => write!(f, "遭遇表里没有 {k}"),
            Unresolved::Inexact { key, note } => write!(f, "{key} 构成含随机（{note}）"),
            Unresolved::NoKernelName { key, monster } => {
                write!(f, "{key} 里的 {monster} 还没有连接键（实录里没见过）")
            }
            Unresolved::NotInKernel { key, monster, kernel_name } => {
                write!(f, "{key} 里的 {monster}（{kernel_name}）内核没建")
            }
        }
    }
}

/// 四个遭遇池（[源码] `ActModel` 的四个 `AllXxxEncounters`）。
///
/// **弱怪和普通怪是两个池子**，不是一个池子的两档：开头几场只从弱怪池抽，
/// 之后只从普通池抽，两边各有各的袋子（`GrabBag`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pool {
    Weak,
    Regular,
    Elite,
    Boss,
}

/// 遭遇表。
pub struct Table {
    pub acts: Vec<Act>,
    pub encounters: BTreeMap<String, Encounter>,
    /// 英文类名 -> 内核敌人的中文名（`null` = 还没有连接键）
    pub kernel_name: BTreeMap<String, Option<String>>,
}

fn arr_of_str(j: Option<&Json>) -> Vec<String> {
    match j {
        Some(Json::Arr(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

impl Table {
    /// 从 `data/` 读两份表。路径是**目录**，好让调用方（和测试）指到别处去。
    pub fn load(dir: &str) -> Result<Table, String> {
        let enc_src = std::fs::read_to_string(format!("{dir}/encounters.json"))
            .map_err(|e| format!("读不了 {dir}/encounters.json: {e}"))?;
        let ids_src = std::fs::read_to_string(format!("{dir}/enemy_ids.json"))
            .map_err(|e| format!("读不了 {dir}/enemy_ids.json: {e}"))?;
        let enc = parse(&enc_src).map_err(|e| format!("{dir}/encounters.json: {e}"))?;
        let ids = parse(&ids_src).map_err(|e| format!("{dir}/enemy_ids.json: {e}"))?;

        let mut acts = Vec::new();
        if let Some(m) = enc.obj("acts") {
            for (name, v) in m {
                acts.push(Act {
                    name: name.clone(),
                    index: v.i64("index").unwrap_or(-1),
                    encounters: arr_of_str(v.get("encounters")),
                    // 缺了就退回 [源码] `ActModel` 的基类默认（3），`base_rooms`
                    // 没有基类默认（抽象属性）—— 拿 0 进去会让整幕链一场都不打，
                    // 那是**静默的空**，所以退回 15（四幕里最大的那个）并不合适：
                    // 真缺的时候 `plan_for` 会因为 `base_rooms < rooms` 而报出来。
                    weak_encounters: v.i64("weak_encounters").unwrap_or(3).max(0) as usize,
                    base_rooms: v.i64("base_rooms").unwrap_or(0).max(0) as usize,
                });
            }
        }
        acts.sort_by_key(|a| a.index);

        let mut encounters = BTreeMap::new();
        if let Some(m) = enc.obj("encounters") {
            for (key, v) in m {
                encounters.insert(
                    key.clone(),
                    Encounter {
                        key: key.clone(),
                        room_type: v.str("room_type").unwrap_or("").to_string(),
                        is_weak: v.get("is_weak").and_then(Json::as_bool).unwrap_or(false),
                        monsters: v.get("monsters").map(|_| arr_of_str(v.get("monsters"))),
                        all_possible: {
                            let mut a = arr_of_str(v.get("all_possible"));
                            if let Some(cf) = v.obj("choice_fields") {
                                for (_, list) in cf {
                                    a.extend(arr_of_str(Some(list)));
                                }
                            }
                            a.sort();
                            a.dedup();
                            a
                        },
                        exact: v.get("exact").and_then(Json::as_bool).unwrap_or(false),
                        note: v.str("note").map(str::to_string),
                        tags: arr_of_str(v.get("tags")),
                    },
                );
            }
        }

        let mut kernel_name = BTreeMap::new();
        if let Some(m) = ids.obj("monsters") {
            for (cls, v) in m {
                kernel_name.insert(cls.clone(), v.str("kernel_enemy").map(str::to_string));
            }
        }
        if encounters.is_empty() || kernel_name.is_empty() {
            return Err(format!("{dir} 里的表是空的 —— 跑一遍 tools/dump_encounters.py"));
        }
        Ok(Table { acts, encounters, kernel_name })
    }

    /// 这一场是不是 Boss 战（`encounters.json` 的 `room_type`）。
    /// 认不出的遭遇一律 `false` —— **方向是低估自己**（缩放仪那 25 点血不给）。
    pub fn is_boss(&self, key: &str) -> bool {
        self.encounters.get(key).map_or(false, |e| e.room_type == "Boss")
    }

    /// 英文类名 -> 内核 `ENEMIES` 的下标。
    pub fn def_of_monster(&self, class_name: &str) -> Option<u16> {
        let kernel = self.kernel_name.get(class_name)?.as_deref()?;
        crate::replay::enemy_id(kernel)
    }

    /// 遭遇 key -> 这一场要放几只什么敌人（血量留给 [`crate::synth::build`] 掷）。
    ///
    /// **构成含随机的整条拒绝**，见模块头。
    pub fn resolve(&self, key: &str) -> Result<Vec<EnemySpec>, Unresolved> {
        let Some(e) = self.encounters.get(key) else {
            return Err(Unresolved::NoSuchEncounter(key.to_string()));
        };
        let Some(monsters) = e.monsters.as_ref().filter(|_| e.exact) else {
            return Err(Unresolved::Inexact {
                key: key.to_string(),
                note: e.note.clone().unwrap_or_else(|| "构成不确定".to_string()),
            });
        };
        let mut out = Vec::new();
        for m in monsters {
            match self.kernel_name.get(m).and_then(|k| k.as_deref()) {
                None => {
                    return Err(Unresolved::NoKernelName {
                        key: key.to_string(),
                        monster: m.clone(),
                    })
                }
                Some(kernel) => match crate::replay::enemy_id(kernel) {
                    Some(def) => out.push(EnemySpec::rolled(def)),
                    None => {
                        return Err(Unresolved::NotInKernel {
                            key: key.to_string(),
                            monster: m.clone(),
                            kernel_name: kernel.to_string(),
                        })
                    }
                },
            }
        }
        Ok(out)
    }

    /// 场上这批敌人**是哪一场遭遇**。给的是内核 def 的多重集，
    /// 返回全部对得上的 key（构成确定的那些优先，见 `exact`）。
    ///
    /// 两种命中分开报，因为它们说的不是一件事：
    /// * `exact` —— 这一场的构成在 [源码] 里就是定死的，认得死
    /// * 只落在 `all_possible` 里 —— 构成含随机，只能说"可能是它"
    pub fn identify(&self, defs: &[u16]) -> (Vec<String>, Vec<String>) {
        let mut want: Vec<u16> = defs.to_vec();
        want.sort_unstable();
        let mut exact_hits = Vec::new();
        let mut loose_hits = Vec::new();
        for (key, e) in &self.encounters {
            if e.exact {
                if let Some(ms) = &e.monsters {
                    let mut got: Vec<u16> =
                        ms.iter().filter_map(|m| self.def_of_monster(m)).collect();
                    got.sort_unstable();
                    if got.len() == ms.len() && got == want {
                        exact_hits.push(key.clone());
                    }
                }
            } else {
                let pool: Vec<u16> =
                    e.all_possible.iter().filter_map(|m| self.def_of_monster(m)).collect();
                if !want.is_empty() && want.iter().all(|d| pool.contains(d)) {
                    loose_hits.push(key.clone());
                }
            }
        }
        (exact_hits, loose_hits)
    }

    /// 按名字找一幕（`"Underdocks"`）。
    pub fn act_by_name(&self, name: &str) -> Option<&Act> {
        self.acts.iter().find(|a| a.name == name)
    }

    /// **这场遭遇属于哪一幕。** 同一个 key 只会出现在一幕里
    /// （`acts[*].encounters` 是 [源码] 逐幕列的），所以第一个命中就是答案。
    pub fn act_of_encounter(&self, key: &str) -> Option<&Act> {
        self.acts.iter().find(|a| a.encounters.iter().any(|k| k == key))
    }

    /// 这一幕的**某一类**遭遇池（[源码] `AllWeakEncounters` /
    /// `AllRegularEncounters` / `AllEliteEncounters` / `AllBossEncounters`）。
    ///
    /// **不过滤"内核开不开得出"** —— 池子是 [源码] 档的事实，
    /// 而"开不出来"是内核这一侧的缺口。两件事混在一起，
    /// 抽出来的序列就会**静默地偏向内核认识的那几场**，
    /// 而整幕链要报的恰恰是「这次评估漏掉了多少」。
    pub fn pool<'a>(&'a self, act: &'a Act, kind: Pool) -> Vec<&'a Encounter> {
        act.encounters
            .iter()
            .filter_map(|k| self.encounters.get(k))
            .filter(|e| match kind {
                Pool::Weak => e.room_type == "Monster" && e.is_weak,
                Pool::Regular => e.room_type == "Monster" && !e.is_weak,
                Pool::Elite => e.room_type == "Elite",
                Pool::Boss => e.room_type == "Boss",
            })
            .collect()
    }

    /// 这一幕**内核今天开得出几场仗**。分子是 [`Table::resolve`] 成功的场数。
    ///
    /// 这个数和 `tools/dump_encounters.py` 的覆盖率报告问的是同一件事，
    /// 但它是**内核这一侧**算的 —— 两边对不上就说明有一侧的连接键读错了。
    pub fn coverage(&self, act: &Act) -> (usize, usize) {
        let ok = act.encounters.iter().filter(|k| self.resolve(k).is_ok()).count();
        (ok, act.encounters.len())
    }
}
