#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import record_trace as rec

ROOT = rec.ROOT
VENV_PY = sys.executable

for stream in (sys.stdout, sys.stderr):
    try:
        stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


def get_st():
    return rec.get_state()


def post_act(body):
    return rec.post_action(body)


def wait_settle(timeout_s=10.0, require_play_phase=False):
    t0 = time.time()
    last = None
    stable = 0
    while time.time() - t0 < timeout_s:
        try:
            st = get_st()
            st_type = st.get("state_type")
            if st_type in rec.COMBAT_STATES:
                battle = st.get("battle") or {}
                # check if play phase is active if required
                if require_play_phase and not battle.get("is_play_phase"):
                    time.sleep(0.1)
                    continue
            summary = (st_type, st.get("player", {}).get("hp"), str(st.get("player", {}).get("hand")))
            if summary == last:
                stable += 1
                if stable >= 3:
                    return st
            else:
                last = summary
                stable = 0
        except Exception:
            pass
        time.sleep(0.1)
    return get_st()


def show_state():
    st = get_st()
    st_type = st.get("state_type")
    player = st.get("player") or {}
    run = st.get("run") or {}
    print(f"=== State: {st_type} | Floor {run.get('floor')} (Act {run.get('act')}) | HP: {player.get('hp')}/{player.get('max_hp')} | Gold: {player.get('gold')} ===")
    
    if st_type in rec.COMBAT_STATES:
        battle = st.get("battle") or {}
        print(f"Combat Turn {battle.get('turn')} | Phase: {'PLAY' if battle.get('is_play_phase') else 'ENEMY/ANIM'}")
        print(f"Player Block: {player.get('block')} | Energy: {player.get('energy')}/{player.get('max_energy')}")
        hand = player.get("hand") or []
        print("--- Hand ---")
        for i, c in enumerate(hand):
            ench = f" [{c['enchantment']['name']}]" if c.get("enchantment") else ""
            print(f"  [{i}] {c.get('name')}{ench} (cost={c.get('cost')}, can_play={c.get('can_play')}, target={c.get('target_type')})")
        enemies = battle.get("enemies") or []
        print("--- Enemies ---")
        for i, e in enumerate(enemies):
            intents = e.get("intents") or []
            intent_str = "; ".join([f"{it.get('type')}:{it.get('label')}" for it in intents])
            powers = ", ".join([f"{p.get('id')}:{p.get('amount')}" for p in e.get("status") or []])
            print(f"  [{i}] {e.get('name')} ({e.get('entity_id')}) | HP: {e.get('hp')}/{e.get('max_hp')} | Block: {e.get('block')} | Intents: [{intent_str}] | Powers: [{powers}]")

    elif st_type == "rewards":
        rewards_data = st.get("rewards") or {}
        if isinstance(rewards_data, dict):
            rewards = rewards_data.get("items") or []
        else:
            rewards = rewards_data
        print("--- Rewards ---")
        for i, r in enumerate(rewards):
            if isinstance(r, dict):
                print(f"  [{i}] {r.get('type')}: {r.get('description') or r.get('name') or r.get('potion_name') or r}")
            else:
                print(f"  [{i}] {r}")

    elif st_type == "card_reward":
        cards = st.get("cards") or (st.get("card_reward") or {}).get("cards") or []
        print("--- Card Reward Choices ---")
        for i, c in enumerate(cards):
            print(f"  [{i}] {c.get('name')} (cost={c.get('cost')}): {c.get('description')}")
        print("  [skip] Skip card reward")

    elif st_type == "map":
        m = st.get("map") or {}
        print(f"Map pos: {m.get('current_position')}")
        opts = m.get("next_options") or []
        print("--- Next Options ---")
        for o in opts:
            print(f"  [{o.get('index')}] ({o.get('col')}, {o.get('row')}) Type: {o.get('type')}")

    elif st_type == "rest_site":
        opts = st.get("rest_options") or []
        print("--- Rest Options ---")
        for i, o in enumerate(opts):
            print(f"  [{i}] {o.get('name') or o.get('id')}: {o.get('description')} (enabled={o.get('enabled')})")

    elif st_type == "shop":
        shop = st.get("shop") or {}
        items = shop.get("items") or []
        print("--- Shop Items ---")
        for it in items:
            cat = it.get("category")
            name = it.get("card_name") or it.get("relic_name") or it.get("potion_name") or cat
            desc = it.get("card_description") or it.get("relic_description") or ""
            cost = f" (cost={it.get('card_cost')})" if it.get("card_cost") is not None else ""
            print(f"  [{it.get('index')}] {cat}: {name}{cost} - {it.get('price')}g (afford={it.get('can_afford')}) {desc}")

    elif st_type == "event":
        evt = st.get("event") or {}
        print(f"Event: {evt.get('event_name') or evt.get('event_id')}")
        if evt.get("body"):
            print(f"{evt.get('body')}")
        opts = evt.get("options") or []
        print("--- Event Options ---")
        for i, o in enumerate(opts):
            title = o.get("title") or o.get("text") or ""
            desc = f" ({o.get('description')})" if o.get("description") else ""
            locked = " [LOCKED]" if o.get("is_locked") else ""
            print(f"  [{i}] {title}{desc}{locked}")

    elif st_type == "card_select":
        grid = st.get("selectable_cards") or []
        print(f"--- Card Selection ({st.get('prompt') or ''}) ---")
        for i, c in enumerate(grid):
            print(f"  [{i}] {c.get('name')}: {c.get('description')}")


def cmd_play(card_idx, target=None):
    body = {"action": "play_card", "card_index": int(card_idx)}
    if target:
        body["target"] = target
    print(f">> Playing card [{card_idx}] target={target}")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=3.0)
    show_state()


def cmd_end():
    print(">> Ending turn...")
    res = post_act({"action": "end_turn"})
    print("Action response:", res)
    wait_settle(timeout_s=15.0, require_play_phase=True)
    show_state()


def cmd_potion(slot, target=None):
    body = {"action": "use_potion", "slot": int(slot)}
    if target:
        body["target"] = target
    print(f">> Using potion slot [{slot}] target={target}")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=3.0)
    show_state()


def cmd_solve():
    solve_script = os.path.join(ROOT, "tools", "solve_now.py")
    subprocess.run([VENV_PY, solve_script])


def cmd_advise(what="deck"):
    advise_script = os.path.join(ROOT, "tools", "advise_core.py")
    subprocess.run([VENV_PY, advise_script, what])


def cmd_claim(index):
    body = {"action": "claim_reward", "index": int(index)}
    print(f">> Claiming reward [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


def cmd_pick_card(index):
    body = {"action": "select_card_reward", "card_index": int(index)}
    print(f">> Picking card reward [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


def cmd_skip_card():
    body = {"action": "skip_card_reward"}
    print(">> Skipping card reward")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


def cmd_proceed():
    body = {"action": "proceed"}
    print(">> Proceeding...")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


def cmd_map(index):
    body = {"action": "choose_map_node", "index": int(index)}
    print(f">> Choosing map node [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=3.0)
    show_state()


def cmd_rest(index):
    body = {"action": "choose_rest_option", "index": int(index)}
    print(f">> Choosing rest option [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=3.0)
    show_state()


def cmd_event(index):
    body = {"action": "choose_event_option", "index": int(index)}
    print(f">> Choosing event option [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=3.0)
    show_state()


def cmd_select_card(index):
    body = {"action": "select_card", "index": int(index)}
    print(f">> Selecting card [{index}]")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


def cmd_confirm():
    body = {"action": "confirm_selection"}
    print(">> Confirming selection")
    res = post_act(body)
    print("Action response:", res)
    wait_settle(timeout_s=2.0)
    show_state()


if __name__ == "__main__":
    args = sys.argv[1:]
    if not args or args[0] == "state":
        show_state()
    elif args[0] == "play":
        target = args[2] if len(args) > 2 else None
        cmd_play(args[1], target)
    elif args[0] == "end":
        cmd_end()
    elif args[0] == "potion":
        target = args[2] if len(args) > 2 else None
        cmd_potion(args[1], target)
    elif args[0] == "solve":
        cmd_solve()
    elif args[0] == "advise":
        cmd_advise(args[1] if len(args) > 1 else "deck")
    elif args[0] == "claim":
        cmd_claim(args[1])
    elif args[0] == "pick":
        cmd_pick_card(args[1])
    elif args[0] == "skip":
        cmd_skip_card()
    elif args[0] == "proceed":
        cmd_proceed()
    elif args[0] == "map":
        cmd_map(args[1])
    elif args[0] == "rest":
        cmd_rest(args[1])
    elif args[0] == "event":
        cmd_event(args[1])
    elif args[0] == "select":
        cmd_select_card(args[1])
    elif args[0] == "confirm":
        cmd_confirm()
    else:
        print(f"Unknown command {args[0]}")
