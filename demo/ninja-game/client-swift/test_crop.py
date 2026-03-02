from PIL import Image
im = Image.open('/Users/avi/Github/SpacetimeDB/ninja_atlas.png').convert('RGBA')
w, h = im.size
cell_w = 256
cell_h = 256

def show_cell(cx, cy):
    c = im.crop((cx*cell_w, cy*cell_h, (cx+1)*cell_w, (cy+1)*cell_h))
    return c.getextrema()[3][1] > 0

for y in range(6):
    s = []
    for x in range(11):
        s.append('X' if show_cell(x, y) else '.')
    print(''.join(s))
