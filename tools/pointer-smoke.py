#!/usr/bin/env python3
"""Opt-in live test: inert CuTarget squares only; no capture, model, or keyboard.
Build CuTarget and install the tested Cu bundle first. Cu must be idle and quit.
Recorded evidence can be revalidated without replaying any physical input.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


def event_type(event):
    name = event.get('type', '')
    raw = re.fullmatch(r'NSEventType\(rawValue: (\d+)\)', name)
    if raw:
        return {1: 'leftMouseDown', 2: 'leftMouseUp', 3: 'rightMouseDown', 4: 'rightMouseUp',
                5: 'mouseMoved', 6: 'leftMouseDragged', 7: 'rightMouseDragged',
                10: 'keyDown', 11: 'keyUp', 12: 'flagsChanged', 22: 'scrollWheel',
                25: 'otherMouseDown', 26: 'otherMouseUp', 27: 'otherMouseDragged'}.get(int(raw[1]), name)
    return name


def validate(directory):
    report = json.loads((directory / 'cursor-feedback.json').read_text())
    events = [json.loads(line) for line in (directory / 'events.jsonl').read_text().splitlines()]
    assert report['status'] == 'done', report
    assert report['actions'] == 7, report
    feedback = report['feedback']
    assert len(feedback) == 15, report
    assert len({event['sequence'] for event in feedback}) == 15
    assert sum(event['kind'] == 'move' for event in feedback) == 7
    assert sum(event['kind'] == 'button_down' for event in feedback) == 4
    assert sum(event['kind'] == 'click' for event in feedback) == 4
    assert report['click_pulses'] == 4
    posted = [e for e in events if e.get('tagged')]
    down_up = [e for e in posted if event_type(e) in ['leftMouseDown', 'leftMouseUp', 'rightMouseDown', 'rightMouseUp']]
    assert len(down_up) == 8, down_up
    assert not any(event_type(e) in ['keyDown', 'keyUp', 'flagsChanged', 'scrollWheel'] for e in posted)
    layout = json.loads((directory / 'layout.json').read_text())
    for event in posted:
        assert event['target'] in ['btn-32', 'btn-48', 'btn-64'], event
        r = layout['targets'][event['target']]
        assert abs(event['x'] - (r['x'] + r['w']/2)) <= 1
        assert abs(event['y'] - (r['y'] + r['h']/2)) <= 1
    assert len(posted) == 15, posted
    # End-to-end match between posted feedback and independently received events.
    expected_kinds = {'mouseMoved': 'move', 'leftMouseDown': 'button_down', 'rightMouseDown': 'button_down',
                      'leftMouseUp': 'click', 'rightMouseUp': 'click'}
    for received, shown in zip(posted, feedback):
        assert expected_kinds[event_type(received)] == shown['kind']
        assert abs(received['x'] - shown['x']) <= 1 and abs(received['y'] - shown['y']) <= 1
        if 'click_count' in received:
            assert received['click_count'] == shown['count']
    assert [e['click_count'] for e in down_up if event_type(e).endswith('Down')] == [1, 1, 2, 1]
    return {'passed': True, 'actions': 7, 'posted_feedback': 15, 'click_pulses': 4, 'target_button_receipts': 8, 'directory': str(directory)}


def live(app):
    root = Path(__file__).resolve().parent.parent
    commands = subprocess.check_output(['/bin/ps', '-axo', 'comm='], text=True).splitlines()
    if str(app / 'Contents/MacOS/Cu') in commands or str(app / 'Contents/Resources/cu-engine') in commands:
        raise SystemExit('Quit idle Cu first; this test never interrupts an existing task.')
    target_bin = root / 'tools/CuTarget/.build/release/CuTarget'
    if not target_bin.is_file():
        raise SystemExit('Build tools/CuTarget in release mode first.')
    directory = Path(tempfile.mkdtemp(prefix='cu-cursor-live-'))  # mode 0700
    print(f'Test artifacts: {directory}', flush=True)
    with (directory / 'target-stdout.log').open('wb') as output:
        target = subprocess.Popen([str(target_bin)], env={**os.environ, 'CUTARGET_DIR': str(directory)}, stdout=output, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 10
            layout_path = directory / 'layout.json'
            while not layout_path.is_file():
                if target.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('CuTarget did not become ready')
                time.sleep(0.1)
            time.sleep(0.5)
            subprocess.run(['/usr/bin/open', '-g', str(app), '--args', '--cursor-test-layout', str(layout_path)], check=True)
            deadline = time.monotonic() + 18
            while not (directory / 'cursor-feedback.json').is_file():
                if time.monotonic() > deadline:
                    raise RuntimeError('Cu pointer test did not finish; inspect the app before retrying')
                time.sleep(0.1)
            return validate(directory)
        finally:
            target.terminate()
            try:
                target.wait(timeout=3)
            except subprocess.TimeoutExpired:
                target.kill()
                target.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--live-inert-targets', action='store_true')
    mode.add_argument('--verify-artifacts', type=Path)
    parser.add_argument('--app', default='/Applications/Cu.app')
    args = parser.parse_args()
    result = validate(args.verify_artifacts) if args.verify_artifacts else live(Path(args.app).resolve())
    print(json.dumps(result))


if __name__ == '__main__':
    main()
