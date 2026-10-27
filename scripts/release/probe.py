"""Create a temporary, policy-blocked App PR, verify CI starts, then close it."""
import base64
import json
import urllib.request
import os
import time
from policy import api, endpoint, require, run

branch = 'codex/release-app-probe-' + os.environ['GITHUB_RUN_ID']
head = api(endpoint('git/ref/heads/main'))['object']['sha']
api(endpoint('git/refs'), 'POST', {'ref': 'refs/heads/' + branch, 'sha': head})
number = None
try:
    result = api(endpoint('contents/docs/release-app-probe.md'), 'PUT', {
        'branch': branch, 'message': 'docs: test release App integration',
        'content': base64.b64encode(b'Temporary release App CI probe. Never merge.\n').decode()})
    sha = result['commit']['sha']
    pr = api(endpoint('pulls'), 'POST', {
        'head': branch, 'base': 'main', 'title': 'Release App probe — DO NOT MERGE',
        'body': 'Temporary rollout verification. The deliberately non-conventional title blocks release policy. This PR closes automatically.'})
    number = pr['number']
    print(pr['html_url'], flush=True)
    # The invalid title keeps the required policy gate closed even if builds finish.
    run('gh', 'pr', 'merge', str(number), '--auto', '--squash', '--match-head-commit', sha)
    for attempt in range(24):
        with urllib.request.urlopen('https://api.github.com/' + endpoint(f'commits/{sha}/check-runs')) as response:
            checks = json.load(response)['check_runs']
        names = {check['name'] for check in checks}
        if {'Build / Linux', 'Build / Windows'} <= names:
            print('App-created PR triggered both platform checks and accepted auto-merge.', flush=True)
            break
        time.sleep(5)
    else:
        raise ValueError('App-created PR did not trigger both platform checks')
finally:
    if number:
        api(endpoint(f'pulls/{number}'), 'PATCH', {'state': 'closed'})
    api(endpoint('git/refs/heads/' + branch), 'DELETE')
