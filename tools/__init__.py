"""工具脚本。**这个 `__init__.py` 只为一件事存在**：让 `advisor/server.py`
能写 `from tools import advise_core`（同仓库内的普通 import，不是 sys.path 注入）。

**里面每个脚本仍然是可以直接跑的脚本**，而且**一律只用标准库** ——
那条规矩保证录制器和实战入口不依赖 MCP（游戏跑着、MCP server 也跑着的时候
它们要能独立工作，不跟它抢连接）。`mcp` 那个包只出现在 `advisor/`。
"""
