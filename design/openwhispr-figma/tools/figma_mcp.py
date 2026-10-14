"""Llamar a una tool del MCP remoto de Figma usando el token OAuth que ya cachea Hermes.

Uso:
  python figma_mcp.py --list
  python figma_mcp.py get_design_context '{"fileKey":"<key>","nodeId":"0:1"}'
"""
import asyncio
import json
import sys

from mcp import ClientSession
from mcp.client.streamable_http import create_mcp_http_client, streamable_http_client

TOKENS = r"C:/Users/usuario/AppData/Local/hermes/mcp-tokens/figma.json"
URL = "https://mcp.figma.com/mcp"


async def main(tool, args):
    token = json.load(open(TOKENS, encoding="utf-8"))["access_token"]
    client = create_mcp_http_client(headers={"Authorization": f"Bearer {token}"})
    async with client:
        async with streamable_http_client(URL, http_client=client) as streams:
            read, write = streams[0], streams[1]
            async with ClientSession(read, write) as s:
                await s.initialize()
                if tool == "--list":
                    for t in (await s.list_tools()).tools:
                        props = json.dumps(t.input_schema, ensure_ascii=False)
                        print(f"{t.name}: {props}")
                    return
                res = await s.call_tool(tool, args)
                for c in res.content:
                    print(getattr(c, "text", c))


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1], json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}))
