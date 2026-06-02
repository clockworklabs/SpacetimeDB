spacetime generate --out-dir ../client-unity/Assets/Scripts/autogen -y --lang cs --module-path ./
spacetime generate --out-dir ../client-godot/module_bindings -y --lang cs --module-path ./
spacetime generate --lang unrealcpp --uproject-dir ../client-unreal --module-path ./ --module-name client_unreal
