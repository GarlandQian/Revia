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
seen = set()
empty = 0
raw_bytes = 0
gzip_bytes = 0
groups = Counter()
for entry in manifest['files']:
    relative = Path(entry['stored'])
    assert not relative.is_absolute() and '..' not in relative.parts
    assert not any(part in {'.git', 'target', 'node_modules', 'desktop', 'run02'}
                   or part.endswith('.app') for part in relative.parts)
    assert entry['stored'] not in seen
    seen.add(entry['stored'])
    encoded = (root / relative).read_bytes()
    assert len(encoded) == entry['storedBytes']
    assert sha(encoded) == entry['storedSha256']
    original = gzip.decompress(encoded)
    assert len(original) == entry['sourceBytes']
    assert sha(original) == entry['sourceSha256']
    if check_sources:
        assert Path(entry['source']).read_bytes() == original
    empty += len(original) == 0
    raw_bytes += len(original)
    gzip_bytes += len(encoded)
    groups[entry['group']] += 1
assert seen == {str(path.relative_to(root)) for path in root.rglob('*.gz')}
for artifact in manifest['packagingArtifacts']:
    assert sha((root / artifact['stored']).read_bytes()) == artifact['sha256']
summary = manifest['summary']
assert len(seen) == summary['originalFiles']
assert empty == summary['emptyOriginalFiles']
assert raw_bytes == summary['rawBytes']
assert gzip_bytes == summary['gzipBytes']
assert dict(groups) == summary['countsByGroup']
print(json.dumps({
    'verifiedAtUtc': datetime.now(timezone.utc).isoformat(),
    'archive': str(root),
    'manifestSha256': sha(manifest_bytes),
    'verifiedOriginalFiles': len(seen),
    'sourceComparisons': len(seen) if check_sources else 0,
    'gzipAndOriginalSha256Matches': len(seen),
    'mismatches': [],
    'rawBytes': raw_bytes,
    'gzipBytes': gzip_bytes,
    'emptyOriginalFiles': empty,
    'countsByGroup': dict(sorted(groups.items())),
    'scope': 'Static archive verification only; no product or GUI test was run.',
}, indent=2))
