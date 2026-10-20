# Insert Mesh: insert lprism.obj as a mesh body (InchMeshUnit), then convert it to a B-rep (faceted) so it can be measured.
import os
path = r"C:/Users/colli/OneDrive/Documents/GitHub/FissionOxide/.claude/worktrees/parity-loop-driver/parity/probes/parameshinsertaligncommand/obj-inch/lprism.obj"
base = root.features.baseFeatures.add()
base.startEdit()
meshes = root.meshBodies.add(path, adsk.fusion.MeshUnits.InchMeshUnit, base)
base.finishEdit()
mesh = meshes.item(0)
inp = root.features.meshConvertFeatures.createInput([mesh])
inp.meshConvertMethodType = adsk.fusion.MeshConvertMethodTypes.FacetedMeshConvertMethodType
root.features.meshConvertFeatures.add(inp)
mesh.isLightBulbOn = False
