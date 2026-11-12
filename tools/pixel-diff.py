#!/usr/bin/env python3
"""Compare a render with a local reference, without rescaling either image."""
import argparse
import json
import math
from PIL import Image, ImageChops, ImageStat


def crop_box(value):
    try:
        x, y, width, height = map(int, value.split(','))
        if min(x, y) < 0 or min(width, height) <= 0:
            raise ValueError
        return x, y, x + width, y + height
    except ValueError as error:
        raise argparse.ArgumentTypeError('crop must be x,y,width,height with positive size') from error


def compare(actual, reference, tolerance):
    if actual.size != reference.size:
        raise ValueError(f'size mismatch: actual {actual.size}, reference {reference.size}; use explicit crops')
    difference = ImageChops.difference(actual, reference)
    histogram = difference.histogram()
    count = actual.width * actual.height
    channels = difference.split()
    maximum = channels[0]
    for channel in channels[1:]:
        maximum = ImageChops.lighter(maximum, channel)
    changed = sum(maximum.histogram()[tolerance + 1:])
    stats = ImageStat.Stat(difference)
    return difference, {
        'width': actual.width,
        'height': actual.height,
        'channels': 'RGBA',
        'tolerance': tolerance,
        'changed_pixels': changed,
        'changed_percent': changed * 100 / count,
        'mean_absolute_error': sum(stats.mean) / 4,
        'root_mean_square_error': math.sqrt(sum(n * n for n in stats.rms) / 4),
        'maximum_channel_error': max(i % 256 for i, n in enumerate(histogram) if n),
    }


def load(path, box):
    with Image.open(path) as source:
        image = source.convert('RGBA')
    if box:
        if box[2] > image.width or box[3] > image.height:
            raise ValueError(f'crop {box} exceeds {path}: {image.size}')
        image = image.crop(box)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('actual')
    parser.add_argument('reference', help='local PNG or WebP, such as the downloaded zlaunch hero')
    parser.add_argument('--actual-crop', type=crop_box)
    parser.add_argument('--reference-crop', type=crop_box)
    parser.add_argument('--tolerance', type=int, default=0, choices=range(256), metavar='0..255')
    parser.add_argument('--max-changed-percent', type=float, default=100)
    parser.add_argument('--diff', default='pixel-diff.png')
    args = parser.parse_args()
    if not 0 <= args.max_changed_percent <= 100:
        parser.error('max-changed-percent must be 0..100')
    try:
        difference, report = compare(load(args.actual, args.actual_crop), load(args.reference, args.reference_crop), args.tolerance)
        # RGBA difference alpha is often zero, which would hide differing RGB pixels.
        difference.convert('RGB').save(args.diff)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    report.update(actual=args.actual, reference=args.reference, actual_crop=args.actual_crop, reference_crop=args.reference_crop)
    print(json.dumps(report, indent=2))
    raise SystemExit(report['changed_percent'] > args.max_changed_percent)


if __name__ == '__main__':
    main()
