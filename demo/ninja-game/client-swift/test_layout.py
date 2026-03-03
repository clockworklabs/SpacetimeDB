from PIL import Image
import sys
im = Image.open('/Users/avi/Github/SpacetimeDB/ninja_atlas.png').convert('RGBA')
w, h = im.size

grid_x = 22
grid_y = 12
cell_w = w // grid_x
cell_h = h // grid_y
print(f"Cell size: {cell_w}x{cell_h}")

for ry in range(grid_y):
    row_chars = []
    for rx in range(grid_x):
        cell = im.crop((rx*cell_w, ry*cell_h, (rx+1)*cell_w, (ry+1)*cell_h))
        extrema = cell.getextrema()
        if extrema[3][1] > 0: # not fully transparent
            row_chars.append("X")
        else:
            row_chars.append(".")
    print("".join(row_chars))
