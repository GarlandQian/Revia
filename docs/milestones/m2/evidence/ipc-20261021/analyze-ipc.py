#!/usr/bin/env python3
import datetime
import gzip
import json
from pathlib import Path
import sys

KEYS = ('rustFootprintMb', 'webContentRssMb', 'webContentFootprintMb',
        'webContentMallocAllocatedMb', 'networkingRssMb', 'networkingFootprintMb', 'gpuRssMb')

def instant(value):
    return datetime.datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp()

def statistics(samples, origin):
    result = {'count': len(samples)}
    for key in KEYS:
        selected = [sample for sample in samples if key in sample]
        if not selected:
            continue
        values = [sample[key] for sample in selected]
        times = [(instant(sample['timestamp']) - origin) / 60 for sample in selected]
        mx, my = sum(times) / len(times), sum(values) / len(values)
        denominator = sum((x - mx) ** 2 for x in times)
        result[key] = {'first': values[0], 'last': values[-1], 'min': min(values), 'max': max(values),
                       'mean': round(my, 4), 'sampleCount': len(values),
                       'slopeMbPerMinute': round(sum((x-mx)*(y-my) for x,y in zip(times,values))/denominator, 4)
                       if denominator else None}
    return result

def analyze(path):
    report = json.loads(path.read_text())
    frontend = report['frontend']
    start = instant(frontend['operationsStartedAt'])
    end = instant(frontend['operationsEndedAt'])
    idle = frontend['idle']
    idle_start, idle_end = instant(idle['startedAt']), instant(idle['endedAt'])
    memory = report['memoryTimeSeries']
    def within(sample, lower, upper):
        return lower <= instant(sample['samplingStartedAt']) <= instant(sample['timestamp']) <= upper
    active = [sample for sample in memory if within(sample, start, end)]
    steady = [sample for sample in memory if within(sample, start + 180, end)]
    resting = [sample for sample in memory if within(sample, idle_start, idle_end)]
    observed = [sample['ipc'] for sample in frontend['samples']]
    checkpoint_observed = [checkpoint[boundary] for checkpoint in frontend['phaseCheckpoints']
                           for boundary in ('ipcBefore', 'ipcAfter')]
    observed += checkpoint_observed + [frontend['ipcAfterOperations'], idle['initial']]
    observed += [sample['ipc'] for sample in idle['samples']] + [frontend['ipcFinal']]
    callbacks = [sample['callbacks'] for sample in observed]
    callback_sizes = [sample['registrySize'] for sample in callbacks if sample['available']]
    phase = report['phaseMemoryTimeSeries']
    return {
        'path': str(path), 'runCompleted': report['runCompleted'], 'errors': report['errors'],
        'binarySha256': report['binarySha256'], 'diagnostic': report['diagnostic'],
        'startedAt': report['startedAt'], 'endedAt': report['endedAt'],
        'reviaPid': report['reviaPid'], 'servicePids': {key: memory[-1][key] for key in
            ('webContentPids', 'networkingPids', 'gpuPids')},
        'activeSeconds': frontend['actualDurationSeconds'], 'operationsWallSeconds': frontend['wallDurationSeconds'],
        'checkpointPauseSeconds': frontend['checkpointPauseSeconds'], 'idleSeconds': idle['actualSeconds'],
        'counts': {key: frontend[key] for key in ('totalCycles', 'totalFileSwitches', 'totalViewSwitches', 'totalScrolls', 'totalJumps')},
        'ipcAfterOperations': frontend['ipcAfterOperations']['fetch'],
        'ipcFinal': frontend['ipcFinal']['fetch'],
        'observations': len(observed), 'initialCallbacks': frontend['ipcInitial']['callbacks'],
        'callbackMin': min(callback_sizes), 'callbackMax': max(callback_sizes),
        'finalCallbacks': frontend['ipcFinal']['callbacks'],
        'maxObservedCallbackAgeMs': max(sample['oldestObservedAgeMs'] or 0 for sample in callbacks),
        'maxObservedNewIdCount': max(sample['newIdCount'] or 0 for sample in callbacks),
        'observationErrors': max(sample['observationErrors'] for sample in observed),
        'activeMemory': statistics(active, start), 'post180sActiveMemory': statistics(steady, start),
        'idleMemory': statistics(resting, idle_start),
        'idleMemoryTimeSeries': [{key: sample[key] for key in (*KEYS, 'timestamp', 'samplingStartedAt') if key in sample}
                                 for sample in resting],
        'idleProbeSamples': [{'elapsedSec': sample['elapsedSec'], 'callbacks': sample['ipc']['callbacks'],
                             'fetch': sample['ipc']['fetch']['total'], 'dom': sample['ipc']['dom'],
                             'resourceTimingEntries': sample['ipc']['resourceTimingEntries']} for sample in idle['samples']],
        'phaseSamples': len(phase), 'validPhaseSamples': sum(sample['valid'] for sample in phase),
        'invalidPhaseErrors': [sample['errors'] for sample in phase if not sample['valid']],
        'visibility': frontend['finalVisibility'],
        'operationLogBytes': (Path(str(path)+'.frontend.log').stat().st_size if Path(str(path)+'.frontend.log').exists()
                              else len(gzip.decompress(Path(str(path)+'.frontend.log.gz').read_bytes()))),
        'acceptance': 'diagnostic only; neither memory-leak proof nor baseline acceptance',
    }

if __name__ == '__main__':
    results = [analyze(Path(path)) for path in sys.argv[1:]]
    print(json.dumps(results, ensure_ascii=False, indent=2))
