import sys
from PIL import Image

image = Image.open(sys.argv[1])
w, h = image.size
image = image.crop((w // 2 - 310, h // 2 - 225, w // 2 + 310, h // 2 + 225))
image = image.resize((image.width * 3, image.height * 3)).convert('L')
image.point(lambda value: 0 if value > 45 else 255).save(sys.argv[2])
