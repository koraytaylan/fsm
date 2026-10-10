"""Operational resource observations fail closed without a frozen calibration."""
from .soak import bounded_integer

METRICS = ('work_latency_ns', 'control_latency_ns', 'scheduler_lag_ns',
           'rss_bytes', 'file_descriptors', 'active_children', 'surviving_descendants',
           'capture_bytes', 'journal_bytes', 'disk_bytes', 'cpu_ns')
GROWTH = ('rss_bytes', 'file_descriptors', 'capture_bytes')


def calibration_for(host, calibrations, workload_digest):
    if not isinstance(calibrations, dict):
        raise ValueError('host calibrations must be a frozen mapping')
    calibration = calibrations.get(host)
    if (not isinstance(calibration, dict) or calibration.get('status') != 'calibrated'
        or calibration.get('workload_sha256') != workload_digest
        or not isinstance(calibration.get('evidence_sha256'), str)
        or len(calibration['evidence_sha256']) != 64
        or any(character not in '0123456789abcdef' for character in calibration['evidence_sha256'])
        or not isinstance(calibration.get('ceilings'), dict)
        or set(calibration['ceilings']) != set(METRICS)
        or not isinstance(calibration.get('quiescent_growth'), dict)
        or set(calibration['quiescent_growth']) != set(GROWTH)):
        raise ValueError('required host/workload calibration is missing or inconsistent')
    for key, value in calibration['ceilings'].items():
        bounded_integer(value, 0, (1 << 63) - 1, 'calibrated ' + key)
    for key, value in calibration['quiescent_growth'].items():
        bounded_integer(value, 0, (1 << 63) - 1, 'quiescent ' + key)
    return calibration


def validate_sample(sample, calibration, warmed=None):
    if (not isinstance(sample, dict) or sample.get('phase') not in ('active', 'quiescent')
        or not isinstance(sample.get('metrics'), dict)
        or set(sample['metrics']) != set(METRICS)):
        raise ValueError('every required resource metric must be observed')
    values = sample['metrics']
    for key in METRICS:
        value = bounded_integer(values[key], 0, (1 << 63) - 1, key)
        if value > calibration['ceilings'][key]:
            raise ValueError('resource ceiling exceeded: ' + key)
    if sample['phase'] == 'quiescent':
        if any(values[key] != 0 for key in ('active_children', 'surviving_descendants', 'capture_bytes')):
            raise ValueError('quiescent owned children, descendants and captures must be retired')
        if warmed is not None:
            validate_sample(warmed, calibration)
            if (warmed.get('phase') != 'quiescent'
                or sample.get('equivalent_state') != warmed.get('equivalent_state')
                or sample.get('equivalent_state') is None):
                raise ValueError('resource growth requires equivalent warmed/quiescent states')
            for key in GROWTH:
                if values[key] - warmed['metrics'][key] > calibration['quiescent_growth'][key]:
                    raise ValueError('quiescent resource growth exceeded: ' + key)
    return sample
