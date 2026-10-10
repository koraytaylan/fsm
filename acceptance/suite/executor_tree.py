"""Original descendant/pipe observations grant no native closure authority."""
import re
import stat

from .executor_scenarios import _fixture_rows, _wait_for_files
from .executor_lifecycle import process_observation

NOISE_BYTES = 131_072


def tree_rows(root, parents):
    children = _fixture_rows(root / 'descendants.jsonl')
    noise = _fixture_rows(root / 'noise.jsonl')
    by_run = {row['run']:row for row in parents if row['operation']=='validate'}
    if len(children) != len(by_run) or len(noise) != len(by_run):
        raise ValueError('each actual validation must have one original descendant and output observation')
    seen = set()
    for child in children:
        parent = by_run.get(child.get('run'))
        if (parent is None or child['run'] in seen or child.get('resource') != parent['resource']
            or type(child.get('pid')) is not int or not 0 < child['pid'] < 1 << 31
            or not isinstance(child.get('pid_starttime'), str) or not re.fullmatch('[0-9]{1,20}',child['pid_starttime'])
            or child.get('parent') != {key:parent[key] for key in ('pid','pid_starttime')}
            or type(child.get('parent_pid')) is not int or child['parent_pid'] != parent['pid']
            or child['pid'] == parent['pid'] or not isinstance(child.get('cgroup'), str)
            or not 1 <= len(child['cgroup']) <= 4096 or child.get('parent_cgroup') != child['cgroup']
            or set(child.get('streams', {})) != {'1','2'} or child.get('parent_streams') != child['streams']):
            raise ValueError('the original descendant has a foreign parent, domain or process identity')
        for material in child['streams'].values():
            if (set(material) != {'device','inode','mode'}
                or any(type(material.get(key)) is not int for key in material)
                or material['device'] < 0 or material['inode'] <= 0
                or not (stat.S_ISREG(material['mode']) or stat.S_ISFIFO(material['mode']) or stat.S_ISSOCK(material['mode']))):
                raise ValueError('the original descendant lacks its inherited capture/protocol pipe identities')
        seen.add(child['run'])
    seen = set()
    for row in noise:
        parent = by_run.get(row.get('run'))
        if (parent is None or row['run'] in seen or set(row) != {'run','bytes','descriptor','pid','pid_starttime'}
            or type(row.get('bytes')) is not int or row['bytes'] != NOISE_BYTES
            or type(row.get('descriptor')) is not int or row['descriptor'] != 2
            or type(row.get('pid')) is not int or row['pid'] != parent['pid']
            or row.get('pid_starttime') != parent['pid_starttime']):
            raise ValueError('the output flood is not one bounded actual write by the original validation')
        seen.add(row['run'])
    return children, noise


def original_tree(report, native, parents):
    children, noise = tree_rows(native.resource, parents)
    if len(parents) != 1 or len(children) != 1:
        raise ValueError('the original exact tree boundary requires one original validation')
    original = parents[0]; child = children[0]
    _wait_for_files(lambda: not process_observation(original)['alive'], native.process, 5)
    dead = process_observation(original); live = process_observation(child)
    report.true(dead['alive'] is False and live['alive'] is True,
        'the original descendant really outlives its exited process/MCP handler at the exact pre-fence cut')
    expected = 'fsm-containment-' + native.namespace + '-1-1.service'
    report.true(child['cgroup'].strip().split('/')[-1] == expected,
        'the inherited-pipe descendant occupies the exact original claimed native domain')
    report.equal(noise[0]['bytes'], NOISE_BYTES,
        'the original handler writes a complete bounded output flood before its genuine result')
    return dict(original_children=children,original_noise=noise,parent_dead=dead,child_before=live)


def tree_before_successor(report, native, parents, proof):
    _wait_for_files(lambda: len(_fixture_rows(native.resource / 'descendants.jsonl')) == 2
        and len(_fixture_rows(native.resource / 'noise.jsonl')) == 2,native.process,5)
    children, noise = tree_rows(native.resource, parents)
    after = process_observation(proof['original_children'][0])
    report.true(after['alive'] is False,
        'the original inherited-pipe descendant is dead before releasing successor validation')
    fresh = [row for row in children if row['run'] != proof['original_children'][0]['run']]
    report.equal(len(fresh),1,'only one fresh validation owns a new real descendant')
    live = process_observation(fresh[0])
    report.true(live['alive'] is True,'the successor descendant is really alive at its own validation barrier')
    proof.update(child_after=after,successor_child_before=live,children_before_successor=children,noise_before_successor=noise)


def final_tree(report, native, parents, proof):
    children, noise = tree_rows(native.resource, parents)
    observations = [process_observation(row) for row in children]
    report.true(all(row['alive'] is False for row in observations),
        'every observed original and successor retained-pipe descendant is dead before final owner drain')
    proof.update(children=children,noise=noise,children_final=observations)
