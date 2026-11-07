// Compositor-independent protocol/lifecycle logic; exercised with a simulated host.
export class Controller {
    constructor(host) {
        this.host = host;
        this.owner = null;
        this.lastSeen = 0;
        this.actions = new Map();
        this.pending = [];
    }

    clear() {
        for (const action of this.actions.keys())
            this.host.ungrab(action);
        this.actions.clear();
        this.pending = [];
        this.owner = null;
    }

    expire() {
        if (this.owner && this.host.now() - this.lastSeen > 15000)
            this.clear();
    }

    activated(action) {
        this.expire();
        if (this.actions.has(action) && this.pending.length < 32)
            this.pending.push(this.actions.get(action));
    }

    call(raw) {
        try {
            this.expire();
            if (raw.length > 65536)
                throw new Error('Bridge request is too large');
            const request = JSON.parse(raw);
            if (request.version !== 1)
                throw new Error('Unsupported HerdCtrl bridge version');
            let result;
            switch (request.operation) {
            case 'state':
                result = {monitors: this.host.monitors(), windows: this.host.windows()};
                break;
            case 'position': {
                const window = this.host.windows().find(w => w.id === request.id && w.pid === request.pid);
                const monitor = this.host.monitors().find(m => m.name === request.monitor);
                if (!window)
                    throw new Error('Window disappeared or its process changed');
                if (!monitor)
                    throw new Error(`Monitor ${request.monitor} is disconnected`);
                const {x, y, width, height} = request;
                if (![x, y, width, height].every(Number.isSafeInteger) || width < 1 || height < 1 ||
                    x < monitor.x || y < monitor.y ||
                    x + width > monitor.x + monitor.width || y + height > monitor.y + monitor.height)
                    throw new Error('Target rectangle is outside the current monitor');
                this.host.position(request);
                result = null;
                break;
            }
            case 'bindings': {
                if (typeof request.token !== 'string' || !request.token || request.token.length > 128)
                    throw new Error('Invalid application token');
                if (this.owner && this.owner !== request.token)
                    throw new Error('Another HerdCtrl instance owns the global shortcuts');
                if (!Array.isArray(request.bindings) || request.bindings.length > 256 ||
                    request.bindings.some(b => !Number.isSafeInteger(b.id) || b.id < 0 ||
                        typeof b.accelerator !== 'string' || !b.accelerator || b.accelerator.length > 128))
                    throw new Error('Invalid shortcut bindings');
                this.clear();
                this.owner = request.token;
                this.lastSeen = this.host.now();
                const errors = [];
                for (const binding of request.bindings) {
                    try {
                        const action = this.host.grab(binding.accelerator);
                        if (action)
                            this.actions.set(action, binding.id);
                        else
                            errors.push(`View ${binding.id + 1}: shortcut ${binding.accelerator} is unavailable`);
                    } catch (error) {
                        errors.push(`View ${binding.id + 1}: ${error.message}`);
                    }
                }
                result = errors;
                break;
            }
            case 'poll':
                if (this.owner !== request.token)
                    throw new Error('Shortcut session expired; reconnect HerdCtrl');
                this.lastSeen = this.host.now();
                result = this.pending.splice(0);
                break;
            case 'release':
                if (this.owner === request.token)
                    this.clear();
                result = null;
                break;
            default:
                throw new Error('Unknown HerdCtrl bridge operation');
            }
            return JSON.stringify({ok: true, result});
        } catch (error) {
            return JSON.stringify({ok: false, error: String(error.message ?? error)});
        }
    }
}
