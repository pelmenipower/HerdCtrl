import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import {Controller} from './controller.mjs';

const XML = `<node><interface name="org.gnome.Shell.Extensions.HerdCtrl">
  <method name="Call"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
</interface></node>`;

export default class HerdCtrlExtension extends Extension {
    enable() {
        this._controller = new Controller({
            now: () => GLib.get_monotonic_time() / 1000,
            monitors: () => this._monitors(),
            windows: () => this._windows().map(w => ({id: String(w.get_id()), pid: w.get_pid()})),
            position: request => this._position(request),
            grab: accelerator => {
                const action = global.display.grab_accelerator(accelerator, Meta.KeyBindingFlags.IGNORE_AUTOREPEAT);
                if (action)
                    Main.wm.allowKeybinding(`external-grab-${action}`, Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW);
                return action;
            },
            ungrab: action => {
                Main.wm.allowKeybinding(`external-grab-${action}`, Shell.ActionMode.NONE);
                global.display.ungrab_accelerator(action);
            },
        });
        try {
            this._exported = Gio.DBusExportedObject.wrapJSObject(XML, {
                Call: request => this._controller.call(request),
            });
            this._exported.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/HerdCtrl');
            this._acceleratorSignal = global.display.connect('accelerator-activated', (_display, action) => {
                this._controller.activated(action);
            });
            this._expirySource = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 5, () => {
                this._controller.expire();
                return GLib.SOURCE_CONTINUE;
            });
        } catch (error) {
            this.disable();
            throw error;
        }
    }

    _monitors() {
        return (global.backend.get_monitor_manager().get_logical_monitors() ?? []).map(logical => {
            const index = logical.get_number();
            const geometry = global.display.get_monitor_geometry(index);
            // Mirrored physical outputs share one logical rectangle.
            const name = logical.get_monitors().map(m => m.get_connector()).sort().join('+');
            // Space left by the top bar and docks, so zones do not cover them.
            const area = global.workspace_manager.get_active_workspace().get_work_area_for_monitor(index);
            return {
                name, index, x: geometry.x, y: geometry.y,
                width: geometry.width, height: geometry.height,
                scale: global.display.get_monitor_scale(index),
                work_area: {x: area.x, y: area.y, width: area.width, height: area.height},
            };
        });
    }

    _windows() {
        return global.display.list_all_windows().filter(w =>
            w.get_pid() > 0 && !w.is_override_redirect() &&
            [Meta.WindowType.NORMAL, Meta.WindowType.DIALOG, Meta.WindowType.MODAL_DIALOG].includes(w.get_window_type()));
    }

    _position(request) {
        const window = this._windows().find(w => String(w.get_id()) === request.id && w.get_pid() === request.pid);
        const monitor = this._monitors().find(m => m.name === request.monitor);
        if (!window || !monitor)
            throw new Error('Window or monitor disappeared');
        window.unmake_fullscreen();
        window.unmaximize();
        window.unminimize();
        window.move_to_monitor(monitor.index);
        window.move_resize_frame(true, request.x, request.y, request.width, request.height);
    }

    disable() {
        if (this._expirySource) {
            GLib.Source.remove(this._expirySource);
            this._expirySource = null;
        }
        if (this._acceleratorSignal) {
            global.display.disconnect(this._acceleratorSignal);
            this._acceleratorSignal = null;
        }
        this._controller?.clear();
        this._controller = null;
        this._exported?.unexport();
        this._exported = null;
    }
}
