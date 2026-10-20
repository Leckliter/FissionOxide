"""FoxBridge: a Fusion add-in that lets the FissionOxide parity loop drive Fusion.

It serves HTTP on 127.0.0.1:47800. Requests carry the token written to
%APPDATA%/FoxBridge/token (~/FoxBridge/token elsewhere) in an `X-Fox-Token` header.

    GET  /ping                 -> {"ok": true, "fusion": version}
    POST /run  {"op": ...}     -> the op's result

Ops (see `dispatch`): catalog, recipe, measure, screenshot, python, close_all.

The Fusion API may only be called on Fusion's main thread, so the server thread queues each
request and fires a custom event; the event handler runs the op and wakes the server thread.
`catalog` and `recipe_runner` are reloaded on every request, so editing them needs no restart.
"""

import importlib
import json
import os
import secrets
import sys
import threading
import traceback
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import adsk.core
import adsk.fusion

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
  sys.path.insert(0, HERE)

PORT = int(os.environ.get("FOXBRIDGE_PORT", "47800"))
EVENT_ID = "FoxBridgeJob"
TIMEOUT_S = 600

_app = adsk.core.Application.get()
_handlers = []
_jobs = {}
_server = None
_token = None


def token_path():
  d = os.path.join(os.environ.get("APPDATA") or os.path.expanduser("~"), "FoxBridge")
  os.makedirs(d, exist_ok=True)
  return os.path.join(d, "token")


def dispatch(req):
  op = req.get("op")
  if op in ("catalog", "recipe", "measure", "screenshot", "close_all"):
    import catalog
    import measure
    import recipe_runner

    for m in (measure, catalog, recipe_runner):
      importlib.reload(m)
    if op == "catalog":
      return catalog.harvest(_app, activate=req.get("activate_workspaces", True))
    if op == "recipe":
      return recipe_runner.run(_app, req["recipe"], keep_open=req.get("keep_open", False),
                               screenshot=req.get("screenshot"))
    if op == "measure":
      return measure.design(_app)
    if op == "screenshot":
      return measure.screenshot(_app, req["path"], req.get("width", 1200), req.get("height", 800),
                                fit=req.get("fit", True))
    if op == "close_all":
      return recipe_runner.close_scratch_documents(_app)
  if op == "python":
    # Runs `code` with app/ui/adsk in scope; the code sets `result`. Lets probe agents reach any
    # part of the API the recipe format doesn't cover. Token-gated and bound to localhost.
    scope = {"app": _app, "ui": _app.userInterface, "adsk": adsk, "result": None}
    exec(req["code"], scope)
    return {"ok": True, "result": scope.get("result")}
  return {"ok": False, "error": f"unknown op {op!r}"}


class _JobHandler(adsk.core.CustomEventHandler):
  def notify(self, args):
    job = _jobs.get(args.additionalInfo)
    if job is None:
      return
    try:
      job["result"] = dispatch(job["req"])
    except Exception:
      job["result"] = {"ok": False, "error": traceback.format_exc()}
    finally:
      job["done"].set()


class _Http(BaseHTTPRequestHandler):
  def log_message(self, *a):
    pass

  def _send(self, code, obj):
    body = json.dumps(obj, default=str).encode()
    self.send_response(code)
    self.send_header("Content-Type", "application/json")
    self.send_header("Content-Length", str(len(body)))
    self.end_headers()
    self.wfile.write(body)

  def _authorized(self):
    if self.headers.get("X-Fox-Token") != _token:
      self._send(403, {"ok": False, "error": "bad or missing X-Fox-Token"})
      return False
    return True

  def do_GET(self):
    if not self._authorized():
      return
    if self.path == "/ping":
      self._send(200, {"ok": True, "fusion": _app.version, "port": PORT})
    else:
      self._send(404, {"ok": False, "error": "not found"})

  def do_POST(self):
    if not self._authorized():
      return
    if self.path != "/run":
      self._send(404, {"ok": False, "error": "not found"})
      return
    try:
      req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
    except ValueError as e:
      self._send(400, {"ok": False, "error": f"bad json: {e}"})
      return
    job_id = uuid.uuid4().hex
    job = {"req": req, "done": threading.Event(), "result": None}
    _jobs[job_id] = job
    try:
      _app.fireCustomEvent(EVENT_ID, job_id)
      if not job["done"].wait(req.get("timeout_s", TIMEOUT_S)):
        self._send(504, {"ok": False, "error": "timed out waiting for Fusion's main thread"})
        return
      self._send(200, job["result"])
    finally:
      _jobs.pop(job_id, None)


def run(context):
  global _server, _token
  try:
    _token = secrets.token_hex(16)
    with open(token_path(), "w") as f:
      f.write(_token)
    event = _app.registerCustomEvent(EVENT_ID)
    h = _JobHandler()
    event.add(h)
    _handlers.append(h)
    _server = ThreadingHTTPServer(("127.0.0.1", PORT), _Http)
    threading.Thread(target=_server.serve_forever, daemon=True).start()
  except Exception:
    _app.userInterface.messageBox("FoxBridge failed to start:\n" + traceback.format_exc())


def stop(context):
  global _server
  try:
    if _server:
      _server.shutdown()
      _server.server_close()
      _server = None
    _app.unregisterCustomEvent(EVENT_ID)
  except Exception:
    pass
