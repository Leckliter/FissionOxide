# Insert Mesh: insert lprism.stl as a mesh body (MillimeterMeshUnit), then convert it to a B-rep (faceted) so it can be measured.
import os
path = r"C:/Users/colli/OneDrive/Documents/GitHub/FissionOxide/.claude/worktrees/parity-loop-driver/parity/probes/parameshinsertaligncommand/stl-millimeter/lprism.stl"
base = root.features.baseFeatures.add()
base.startEdit()
meshes = root.meshBodies.add(path, adsk.fusion.MeshUnits.MillimeterMeshUnit, base)
base.finishEdit()
mesh = meshes.item(0)
inp = root.features.meshConvertFeatures.createInput([mesh])
inp.meshConvertMethodType = adsk.fusion.MeshConvertMethodTypes.FacetedMeshConvertMethodType
root.features.meshConvertFeatures.add(inp)
mesh.isLightBulbOn = False
