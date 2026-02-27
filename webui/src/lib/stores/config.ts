import { derived, writable } from 'svelte/store';
import { createDefaultConfig, createDefaultRoute, createDefaultUpstream, type PrxConfig } from '../types/config';
import { encodeToml } from '../configCodec';

export const configStore = writable<PrxConfig>(createDefaultConfig());

export const tomlPreview = derived(configStore, ($config) => encodeToml($config));

export const validationIssues = derived(configStore, ($config) => {
  const issues: string[] = [];
  if ($config.routes.length === 0) {
    issues.push('ต้องมี route อย่างน้อย 1 รายการ');
  }

  if (!$config.server.health_path.startsWith('/')) {
    issues.push('server.health_path ต้องขึ้นต้นด้วย /');
  }
  if (!$config.server.ready_path.startsWith('/')) {
    issues.push('server.ready_path ต้องขึ้นต้นด้วย /');
  }
  if ($config.server.health_path === $config.server.ready_path) {
    issues.push('server.health_path และ server.ready_path ต้องไม่เหมือนกัน');
  }
  if ($config.server.tls) {
    if (!$config.server.tls.listen.trim()) {
      issues.push('server.tls.listen ห้ามว่าง');
    }
    if (!$config.server.tls.cert_path.trim()) {
      issues.push('server.tls.cert_path ห้ามว่าง');
    }
    if (!$config.server.tls.key_path.trim()) {
      issues.push('server.tls.key_path ห้ามว่าง');
    }
  }

  const defaultCount = $config.routes.filter((route) => route.is_default).length;
  if (defaultCount > 1) {
    issues.push('กำหนด route ที่เป็น default ได้เพียง 1 รายการ');
  }

  $config.routes.forEach((route, index) => {
    if (!route.path_prefix.startsWith('/')) {
      issues.push(`Route #${index + 1}: path_prefix ต้องขึ้นต้นด้วย /`);
    }
    if (route.upstreams.length === 0) {
      issues.push(`Route #${index + 1}: ต้องมี upstream อย่างน้อย 1 รายการ`);
    }
    if (route.circuit_breaker.enabled) {
      if (route.circuit_breaker.consecutive_failures <= 0) {
        issues.push(
          `Route #${index + 1}: circuit_breaker.consecutive_failures ต้องมากกว่า 0`
        );
      }
      if (route.circuit_breaker.open_ms <= 0) {
        issues.push(`Route #${index + 1}: circuit_breaker.open_ms ต้องมากกว่า 0`);
      }
    }
    route.upstreams.forEach((upstream, uidx) => {
      if (!upstream.addr.trim()) {
        issues.push(`Route #${index + 1} Upstream #${uidx + 1}: addr ห้ามว่าง`);
      }
      if (upstream.weight < 1 || upstream.weight > 256) {
        issues.push(`Route #${index + 1} Upstream #${uidx + 1}: weight ต้องอยู่ในช่วง 1..256`);
      }
    });
  });

  return issues;
});

export const resetConfig = (): void => {
  configStore.set(createDefaultConfig());
};

export const addRoute = (): void => {
  configStore.update((config) => {
    config.routes.push(createDefaultRoute(config.routes.length + 1));
    return config;
  });
};

export const removeRoute = (index: number): void => {
  configStore.update((config) => {
    config.routes.splice(index, 1);
    if (config.routes.length === 0) {
      config.routes.push(createDefaultRoute(1));
    }
    return config;
  });
};

export const addUpstream = (routeIndex: number): void => {
  configStore.update((config) => {
    config.routes[routeIndex]?.upstreams.push(createDefaultUpstream());
    return config;
  });
};

export const removeUpstream = (routeIndex: number, upstreamIndex: number): void => {
  configStore.update((config) => {
    const route = config.routes[routeIndex];
    if (!route) {
      return config;
    }
    route.upstreams.splice(upstreamIndex, 1);
    if (route.upstreams.length === 0) {
      route.upstreams.push(createDefaultUpstream());
    }
    return config;
  });
};
