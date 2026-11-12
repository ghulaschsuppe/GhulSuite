#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Minimal SMTP test server for the GhulMail tests (never used by the app).

STARTTLS with the given certificate, AUTH PLAIN/LOGIN with the password test-pw-123,
delivery of every message into <maildir-root>/<recipient>/new as Maildir file.
"""
import asyncio, base64, os, ssl, sys, time, uuid

HOST, PORT, CERT, KEY, ROOT = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5]
PASSWORD = "test-pw-123"
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(CERT, KEY)


def deliver(rcpt, data):
    box = os.path.join(ROOT, rcpt)
    for sub in ("new", "cur", "tmp"):
        os.makedirs(os.path.join(box, sub), exist_ok=True)
    name = f"{int(time.time())}.{uuid.uuid4().hex}.ghultest"
    tmp = os.path.join(box, "tmp", name)
    with open(tmp, "wb") as f:
        f.write(data)
    os.rename(tmp, os.path.join(box, "new", name))


async def handle(reader, writer):
    state = {"tls": False, "auth": False, "from": None, "rcpt": []}

    async def send(line):
        writer.write((line + "\r\n").encode())
        await writer.drain()

    async def readline():
        line = await reader.readline()
        if not line:
            raise ConnectionError
        return line.decode(errors="replace").rstrip("\r\n")

    await send("220 localhost GhulSuite test SMTP")
    try:
        while True:
            line = await readline()
            cmd = line.split(" ", 1)[0].upper()
            arg = line[len(cmd):].strip()
            if cmd in ("EHLO", "HELO"):
                caps = ["250-localhost"]
                if not state["tls"]:
                    caps.append("250-STARTTLS")
                else:
                    caps.append("250-AUTH PLAIN LOGIN")
                caps.append("250 8BITMIME")
                for c in caps:
                    await send(c)
            elif cmd == "STARTTLS":
                await send("220 Ready to start TLS")
                await writer.start_tls(ctx)
                state["tls"] = True
            elif cmd == "AUTH":
                if not state["tls"]:
                    await send("530 Must issue a STARTTLS command first")
                    continue
                parts = arg.split(" ")
                mech = parts[0].upper()
                if mech == "PLAIN":
                    blob = parts[1] if len(parts) > 1 else None
                    if blob is None:
                        await send("334 ")
                        blob = await readline()
                    pw = base64.b64decode(blob).split(b"\0")[-1].decode()
                elif mech == "LOGIN":
                    if len(parts) > 1:
                        base64.b64decode(parts[1])
                    else:
                        await send("334 VXNlcm5hbWU6")
                        await readline()
                    await send("334 UGFzc3dvcmQ6")
                    pw = base64.b64decode(await readline()).decode()
                else:
                    await send("504 Unrecognized authentication type")
                    continue
                if pw == PASSWORD:
                    state["auth"] = True
                    await send("235 Authentication successful")
                else:
                    await send("535 Authentication credentials invalid")
            elif cmd == "MAIL":
                if not state["auth"]:
                    await send("530 Authentication required")
                    continue
                state["from"], state["rcpt"] = arg, []
                await send("250 OK")
            elif cmd == "RCPT":
                addr = arg.split(":", 1)[1].strip().strip("<>").split(">")[0]
                state["rcpt"].append(addr)
                await send("250 OK")
            elif cmd == "DATA":
                await send("354 End data with <CR><LF>.<CR><LF>")
                lines = []
                while True:
                    raw = await reader.readline()
                    if raw in (b".\r\n", b".\n") or not raw:
                        break
                    if raw.startswith(b".."):
                        raw = raw[1:]
                    lines.append(raw)
                data = b"".join(lines)
                for r in state["rcpt"]:
                    deliver(r, data)
                await send("250 OK queued")
            elif cmd == "RSET":
                state["from"], state["rcpt"] = None, []
                await send("250 OK")
            elif cmd == "NOOP":
                await send("250 OK")
            elif cmd == "QUIT":
                await send("221 Bye")
                break
            else:
                await send("502 Command not implemented")
    except (ConnectionError, ssl.SSLError):
        pass
    finally:
        try:
            writer.close()
        except Exception:
            pass


async def main():
    server = await asyncio.start_server(handle, HOST, PORT)
    async with server:
        await server.serve_forever()

asyncio.run(main())
