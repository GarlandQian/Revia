from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import json
from pathlib import Path
import stat
import sys


ROOT = Path(__file__).resolve().parent
CHECK_SOURCES = '--check-sources' in sys.argv[1:]
FORBIDDEN = {'.git', '__pycache__', 'cargo', 'fixture', 'fixtures', 'home',
             'isolated-home', 'node_modules', 'target'}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


manifest_raw = (ROOT / 'manifest.json').read_bytes()
manifest = json.loads(manifest_raw)
assert (ROOT / 'manifest.sha256').read_text().strip() == sha(manifest_raw) + '  manifest.json'
seen = set()
groups = Counter()
raw_bytes = gzip_bytes = empty = 0
for item in manifest['files']:
    relative = Path(item['stored'])
    assert not relative.is_absolute() and '..' not in relative.parts
    assert not any(p in FORBIDDEN or p.endswith('.app') for p in relative.parts)
    assert item['stored'] not in seen
    seen.add(item['stored'])
    stored = ROOT / relative
    assert stat.S_ISREG(stored.lstat().st_mode)
    encoded = stored.read_bytes()
    assert len(encoded) == item['storedBytes'] and sha(encoded) == item['storedSha256']
    decoded = gzip.decompress(encoded)
    assert len(decoded) == item['sourceBytes'] and sha(decoded) == item['sourceSha256']
    if CHECK_SOURCES:
        original = Path(item['source'])
        assert stat.S_ISREG(original.lstat().st_mode)
        assert original.read_bytes() == decoded
    raw_bytes += len(decoded)
    gzip_bytes += len(encoded)
    empty += len(decoded) == 0
    groups[item['group']] += 1
assert seen == {str(path.relative_to(ROOT)) for path in ROOT.rglob('*.gz')}
for item in manifest['packagingArtifacts']:
    assert sha((ROOT / item['stored']).read_bytes()) == item['sha256']
summary = manifest['summary']
assert len(seen) == summary['originalFiles']
assert raw_bytes == summary['rawBytes'] and gzip_bytes == summary['gzipBytes']
assert empty == summary['emptyOriginalFiles'] and dict(groups) == summary['countsByGroup']
print(json.dumps({
    'verifiedAtUtc': datetime.now(timezone.utc).isoformat(),
    'archive': str(ROOT), 'manifestSha256': sha(manifest_raw),
    'verifiedOriginalFiles': len(seen),
    'sourceComparisons': len(seen) if CHECK_SOURCES else 0,
    'gzipAndOriginalSha256Matches': len(seen), 'mismatches': [],
    'rawBytes': raw_bytes, 'gzipBytes': gzip_bytes, 'emptyOriginalFiles': empty,
    'countsByGroup': dict(sorted(groups.items())),
    'scope': 'Static archive verification only; no product, CI, benchmark or GUI test was run.',
}, indent=2))
