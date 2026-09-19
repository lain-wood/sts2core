import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import record_trace as rec

st = rec.get_state()
m = st.get("map", {})
nodes = m.get("nodes", [])
print(f"Current position: {m.get('current_position')}")
print(f"Next options: {json.dumps(m.get('next_options'), ensure_ascii=False)}")
# Trace all paths from next_options to Boss
adj = {}
for n in nodes:
    adj[(n["col"], n["row"])] = n

# Find all paths
paths = []

def dfs(node, current_path):
    if node.get("row") == 15 or node.get("type") == "Boss":
        paths.append(current_path)
        return
    children = node.get("children") or []
    if not children:
        return
    for c_col, c_row in children:
        target = adj.get((c_col, c_row))
        if target:
            dfs(target, current_path + [target])

for opt in m.get("next_options", []):
    opt_node = adj.get((opt["col"], opt["row"]))
    if opt_node:
        dfs(opt_node, [opt_node])

print(f"Total paths to boss: {len(paths)}")
for idx, p in enumerate(paths):
    types = [n["type"] for n in p]
    counts = {}
    for t in types:
        counts[t] = counts.get(t, 0) + 1
    short_str = "".join([n["type"][0] for n in p])
    cols_str = "->".join([f"{n['col']}:{n['type'][0]}" for n in p])
    first_opt = [o["index"] for o in m.get("next_options") if o["col"] == p[0]["col"]][0]
    print(f"Path #{idx:2d} (Opt {first_opt}): Rest={counts.get('RestSite', 0)} Elite={counts.get('Elite', 0)} Shop={counts.get('Shop', 0)} Monster={counts.get('Monster', 0)} Unknown={counts.get('Unknown', 0)} | {cols_str}")

