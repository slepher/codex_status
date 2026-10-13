"""Compare every decoded frame and timing; run with the existing WSL Pillow."""
import hashlib
import json
import sys
from PIL import Image


def inventory(path):
    with Image.open(path) as image:
        frames = []
        loop = image.info.get("loop")
        for index in range(image.n_frames):
            image.seek(index)
            frames.append({"size": image.size,
                           "duration": image.info.get("duration", 0),
                           "rgba_sha256": hashlib.sha256(image.convert("RGBA").tobytes()).hexdigest()})
        return {"loop": loop, "frames": frames}


if __name__ == "__main__":
    original, optimized = map(inventory, sys.argv[1:3])
    if original != optimized:
        raise SystemExit("GIF frame/timing mismatch")
    print(json.dumps({"identical_decoded_frames_and_timing": True,
                      "frames": len(original["frames"]), "loop": original["loop"]}))
