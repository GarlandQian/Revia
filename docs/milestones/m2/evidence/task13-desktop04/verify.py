from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import json
from pathlib import Path
import sys


root = Path(__file__).resolve().parent
check_sources = '--check-sources' in sys.argv[1:]


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


manifest_bytes = (root / 'manifest.json').read_bytes()
manifest = json.loads(manifest_bytes)
assert (root / 'manifest.sha256').read_text().strip() == sha(manifest_bytes) + '  manifest.json'
fixture = Path(manifest['excludedLiveFixtureRoot'])
seen = set()
empty = 0
raw_bytes = 0
stored_bytes = 0
image_files = 0
png_named_files = 0
media_types = Counter()
gzip_files = 0
groups = Counter()
for item in manifest['files']:
    relative = Path(item['stored'])
    source = Path(item['source'])
    assert not relative.is_absolute() and '..' not in relative.parts
    assert not source.is_relative_to(fixture)
    assert not any(part in {'.git', 'target', 'node_modules', '__pycache__'}
                   or part.endswith('.app') for part in relative.parts)
    assert item['stored'] not in seen
    seen.add(item['stored'])
    encoded = (root / relative).read_bytes()
    assert len(encoded) == item['storedBytes']
    assert sha(encoded) == item['storedSha256']
    original = gzip.decompress(encoded) if item['gzip'] else encoded
    if not item['gzip']:
        media_type = ('image/png' if original.startswith(b'\x89PNG\r\n\x1a\n') else
                      'image/jpeg' if original.startswith(b'\xff\xd8\xff') else None)
        assert media_type is not None and item['detectedImageMediaType'] == media_type
        mismatch = relative.suffix.lower() == '.png' and media_type != 'image/png'
        assert item['imageExtensionMismatch'] == mismatch
        image_files += 1
        png_named_files += relative.suffix.lower() == '.png'
        media_types[media_type] += 1
    else:
        gzip_files += 1
    assert len(original) == item['sourceBytes']
    assert sha(original) == item['sourceSha256']
    if check_sources:
        assert source.read_bytes() == original
    empty += len(original) == 0
    raw_bytes += len(original)
    stored_bytes += len(encoded)
    groups[item['group']] += 1
assert seen == {str(path.relative_to(root)) for group in ('desktop-5f', 'desktop-2a51')
                for path in (root / group).rglob('*') if path.is_file()}
for artifact in manifest['packagingArtifacts']:
    assert sha((root / artifact['stored']).read_bytes()) == artifact['sha256']
summary = manifest['summary']
assert len(seen) == summary['originalFiles']
assert empty == summary['emptyOriginalFiles']
assert raw_bytes == summary['rawBytes']
assert stored_bytes == summary['storedBytes']
assert image_files == summary['imageFiles']
assert png_named_files == summary['pngNamedFiles']
assert dict(media_types) == summary['imageMediaTypeCounts']
assert gzip_files == summary['gzipFiles']
assert dict(groups) == summary['countsByGroup']
print(json.dumps({
    'verifiedAtUtc': datetime.now(timezone.utc).isoformat(), 'archive': str(root),
    'manifestSha256': sha(manifest_bytes), 'verifiedOriginalFiles': len(seen),
    'sourceComparisons': len(seen) if check_sources else 0,
    'storedAndOriginalSha256Matches': len(seen), 'mismatches': [],
    'rawBytes': raw_bytes, 'storedBytes': stored_bytes,
    'imageFiles': image_files, 'pngNamedFiles': png_named_files,
    'imageMediaTypeCounts': dict(sorted(media_types.items())),
    'gzipFiles': gzip_files, 'emptyOriginalFiles': empty,
    'countsByGroup': dict(sorted(groups.items())),
    'scope': 'Original desktop evidence byte verification only; no product/GUI test or live-fixture read.',
}, indent=2))
