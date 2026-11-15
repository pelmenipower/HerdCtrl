// CI-only D-Bus service. It simulates a compositor; it does not control windows.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Controller} from '../gnome-extension/herdctrl@herdctrl.local/controller.mjs';

const controller = new Controller({
    now: () => GLib.get_monotonic_time() / 1000,
    monitors: () => [{name: 'DP-1', x: -1280, y: 0, width: 1280, height: 720, scale: 1.5}],
    windows: () => [{id: '42', pid: 123}],
    position: () => {},
    grab: () => 10,
    ungrab: () => {},
});
const xml = `<node><interface name="org.gnome.Shell.Extensions.HerdCtrl">
<method name="Call"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
</interface></node>`;
const exported = Gio.DBusExportedObject.wrapJSObject(xml, {Call: raw => controller.call(raw)});
const loop = new GLib.MainLoop(null, false);
Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnome.Shell', Gio.BusNameOwnerFlags.NONE,
    connection => exported.export(connection, '/org/gnome/Shell/Extensions/HerdCtrl'),
    () => print('Mock GNOME bridge ready'),
    () => { throw new Error('Could not own isolated mock bus name'); });
loop.run();
