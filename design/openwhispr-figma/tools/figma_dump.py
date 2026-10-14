"""Volcar un archivo de Figma a disco via MCP remoto (create_session + N tools).

Uso:
  python figma_dump.py <fileKey> <outdir> '[{"tool":"get_metadata","args":{"nodeId":"0:1"}}]'
Guarda cada salida en <outdir>/<n>-<tool>.txt (o .png si es imagen).
"""
import asyncio
import base64
import json
import pathlib
import sys

from mcp import ClientSession
from mcp.client.streamable_http import create_mcp_http_client, streamable_http_client

TOKENS = r"C:/Users/usuario/AppData/Local/hermes/mcp-tokens/figma.json"
URL = "https://mcp.figma.com/mcp"


async def main(file_key, outdir, calls):
    out = pathlib.Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    token = json.load(open(TOKENS, encoding="utf-8"))["access_token"]
    client = create_mcp_http_client(headers={"Authorization": f"Bearer {token}"})
    async with client:
        async with streamable_http_client(URL, http_client=client) as streams:
            async with ClientSession(streams[0], streams[1]) as s:
                await s.initialize()
                res = await s.call_tool(
                    "create_session",
                    {"fileKey": file_key, "description": "Exportar diseno Open Whispr a especificacion para agentes"},
                )
                sid = json.loads(res.content[0].text)
                sid = sid.get("sessionId") or sid.get("session_id") or str(sid)
                print("session:", sid)
                for i, call in enumerate(calls, 1):
                    args = {"sessionId": sid, "fileKey": file_key, **call.get("args", {})}
                    r = await s.call_tool(call["tool"], args)
                    texts, imgs = [], []
                    for c in r.content:
                        if getattr(c, "type", "") == "image":
                            imgs.append(c.data)
                        else:
                            texts.append(getattr(c, "text", str(c)))
                    p = out / f"{i:02d}-{call['tool']}.txt"
                    p.write_text("\n".join(texts), encoding="utf-8")
                    print(f"{call['tool']} -> {p} ({p.stat().st_size} B, imgs={len(imgs)})")
                    for j, d in enumerate(imgs, 1):
                        ip = out / f"{i:02d}-{call['tool']}-{j}.png"
                        ip.write_bytes(base64.b64decode(d.split(",")[-1]))
                        print("   img ->", ip, ip.stat().st_size, "B")


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1], sys.argv[2], json.loads(sys.argv[3])))
