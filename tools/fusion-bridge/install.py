#!/usr/bin/env python3
"""Links the FoxBridge add-in into Fusion's AddIns folder, so edits here take effect in Fusion.

    py tools/fusion-bridge/install.py            # link (a directory junction on Windows)
    py tools/fusion-bridge/install.py --remove

Then in Fusion: Utilities > Add-Ins > Scripts and Add-Ins > Add-Ins tab > FoxBridge > Run (tick
"Run on Startup" to keep it). It starts automatically on later launches.
"""

import os
import subprocess
import sys

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "FoxBridge")


def addins_dir():
  if os.name == "nt":
    return os.path.join(os.environ["APPDATA"], "Autodesk", "Autodesk Fusion 360", "API", "AddIns")
  return os.path.expanduser("~/Library/Application Support/Autodesk/Autodesk Fusion 360/API/AddIns")


def main():
  dst = os.path.join(addins_dir(), "FoxBridge")
  if "--remove" in sys.argv:
    if os.path.lexists(dst):
      os.rmdir(dst) if os.name == "nt" else os.unlink(dst)  # removes the junction, not the source
      print(f"removed {dst}")
    return
  if os.path.lexists(dst):
    print(f"already installed: {dst}")
    return
  os.makedirs(os.path.dirname(dst), exist_ok=True)
  if os.name == "nt":
    subprocess.run(["cmd", "/c", "mklink", "/J", dst, SRC], check=True)
  else:
    os.symlink(SRC, dst)
  print(f"linked {dst} -> {SRC}")


if __name__ == "__main__":
  main()
