"""Persistent physical fixture identities are observations, never closure authority."""
import re

from .executor_scenarios import _fixture_rows


def fixture_entries(root):
    rows = _fixture_rows(root / 'entries.jsonl')
    for row in rows:
        if (set(row) != {'run', 'resource', 'operation', 'pid', 'pid_starttime'}
            or not isinstance(row['run'], str) or not row['run']
            or row['resource'] != 'supplier' or row['operation'] not in ('validate', 'suspend', 'process', 'restore')
            or type(row['pid']) is not int or not 0 < row['pid'] < 1 << 31
            or not isinstance(row['pid_starttime'], str) or not re.fullmatch('[0-9]{1,20}', row['pid_starttime'])):
            raise ValueError('the original fixture entry log has an invalid process identity')
    if len({row['run'] for row in rows}) != len(rows):
        raise ValueError('the original fixture entry log duplicates an invocation')
    return rows


def validation_entries(root, original_run=None):
    """Read the shared entry slot which survives DynamicUser retirement."""
    return [row for row in fixture_entries(root)
            if row['operation'] == 'validate' and row['run'] != original_run]
