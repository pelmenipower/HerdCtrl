import {test} from 'node:test';
import assert from 'node:assert/strict';
import {Controller} from '../gnome-extension/herdctrl@herdctrl.local/controller.mjs';

function fixture() {
    const host = {
        clock: 0, nextAction: 10, grabbed: new Map(), moved: [], ungrabbed: [],
        connected: true,
        now() { return this.clock; },
        monitors() { return this.connected ? [{name: 'DP-1', x: -1280, y: 0, width: 1280, height: 720, scale: 1.5}] : []; },
        windows() { return [{id: '42', pid: 123}, {id: '43', pid: 999}]; },
        position(request) { this.moved.push(request); },
        grab(accelerator) {
            if (accelerator === '<Super>space' || [...this.grabbed.values()].includes(accelerator))
                return 0;
            const action = this.nextAction++;
            this.grabbed.set(action, accelerator);
            return action;
        },
        ungrab(action) { this.grabbed.delete(action); this.ungrabbed.push(action); },
    };
    const controller = new Controller(host);
    const call = request => JSON.parse(controller.call(JSON.stringify({version: 1, ...request})));
    return {host, controller, call};
}

test('reports logical geometry without multiplying the monitor scale', () => {
    const {call} = fixture();
    const state = call({operation: 'state'});
    assert.equal(state.ok, true);
    assert.equal(state.result.monitors[0].width, 1280);
    assert.equal(state.result.monitors[0].scale, 1.5);
});

test('positions only the requested window with its matching PID', () => {
    const {call, host} = fixture();
    const request = {operation: 'position', id: '42', pid: 123, monitor: 'DP-1', x: -1280, y: 0, width: 640, height: 720};
    assert.equal(call(request).ok, true);
    assert.equal(host.moved.length, 1);
    assert.equal(host.moved[0].id, '42');
    assert.equal(call({...request, pid: 999}).ok, false);
    assert.equal(call({...request, id: 'missing'}).ok, false);
    assert.equal(host.moved.length, 1);
});

test('rejects disconnected monitors and stale or invalid rectangles', () => {
    const {call, host} = fixture();
    const request = {operation: 'position', id: '42', pid: 123, monitor: 'DP-1', x: -1280, y: 0, width: 640, height: 720};
    for (const patch of [{width: 0}, {height: -1}, {x: -1281}, {x: 0}, {height: 721}, {width: 0.5}, {monitor: 'HDMI-2'}])
        assert.equal(call({...request, ...patch}).ok, false);
    host.connected = false;
    assert.equal(call(request).ok, false);
    assert.equal(host.moved.length, 0);
});

test('shortcut grabs report conflicts, queue triggers, and release after heartbeat expiry', () => {
    const {call, host, controller} = fixture();
    const bindings = [{id: 0, accelerator: '<Super>d'}, {id: 1, accelerator: '<Super>space'}];
    const registered = call({operation: 'bindings', token: 'app', bindings});
    assert.equal(registered.ok, true);
    assert.equal(registered.result.length, 1);
    controller.activated(10);
    controller.activated(999);
    assert.deepEqual(call({operation: 'poll', token: 'app'}).result, [0]);
    assert.deepEqual(call({operation: 'poll', token: 'app'}).result, []);
    host.clock = 14000;
    call({operation: 'poll', token: 'app'});
    host.clock = 20000;
    controller.expire();
    assert.equal(host.grabbed.size, 1);
    host.clock = 30000;
    controller.expire();
    assert.equal(host.grabbed.size, 0);
    assert.equal(call({operation: 'poll', token: 'app'}).ok, false);
});

test('another instance cannot replace or drain shortcuts', () => {
    const {call, host} = fixture();
    const bindings = [{id: 0, accelerator: '<Super>d'}];
    call({operation: 'bindings', token: 'first', bindings});
    assert.equal(call({operation: 'bindings', token: 'second', bindings}).ok, false);
    assert.equal(call({operation: 'poll', token: 'second'}).ok, false);
    call({operation: 'release', token: 'second'});
    assert.equal(host.grabbed.size, 1);
    call({operation: 'release', token: 'first'});
    assert.equal(host.grabbed.size, 0);
});

test('reconfiguration and disable discard stale shortcut events', () => {
    const {call, host, controller} = fixture();
    call({operation: 'bindings', token: 'app', bindings: [{id: 0, accelerator: '<Super>d'}]});
    controller.activated(10);
    call({operation: 'bindings', token: 'app', bindings: [{id: 2, accelerator: '<Super>e'}]});
    controller.activated(10);
    controller.activated(11);
    assert.deepEqual(call({operation: 'poll', token: 'app'}).result, [2]);
    controller.clear();
    assert.equal(host.grabbed.size, 0);
    assert.deepEqual(controller.pending, []);
});

test('rejects malformed protocols before changing bindings', () => {
    const {call, controller, host} = fixture();
    call({operation: 'bindings', token: 'app', bindings: [{id: 0, accelerator: '<Super>d'}]});
    assert.equal(JSON.parse(controller.call('not json')).ok, false);
    assert.equal(call({version: 2, operation: 'state'}).ok, false);
    assert.equal(call({operation: 'unknown'}).ok, false);
    assert.equal(call({operation: 'bindings', token: 'app', bindings: [{id: -1, accelerator: 'x'}]}).ok, false);
    assert.equal(host.grabbed.size, 1);
});
