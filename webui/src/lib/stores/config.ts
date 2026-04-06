import { derived, writable } from 'svelte/store';
import {
  createDefaultConfig,
  createDefaultRoute,
  createDefaultService,
  createDefaultUpstream,
  type PrxConfig
} from '../types/config';
import { encodeToml } from '../configCodec';

export const configStore = writable<PrxConfig>(createDefaultConfig());

export const tomlPreview = derived(configStore, ($config) => encodeToml($config));

export const validationIssues = derived(configStore, ($config) => {
  const issues: string[] = [];

  if ($config.services.length === 0) {
    issues.push('ต้องมี service อย่างน้อย 1 รายการ');
  }

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

  const serviceNames = new Set<string>();
  const duplicateServiceNames = new Set<string>();

  $config.services.forEach((service, index) => {
    if (!service.name.trim()) {
      issues.push(`Service #${index + 1}: name ห้ามว่าง`);
    }
    if (serviceNames.has(service.name)) {
      duplicateServiceNames.add(service.name);
    }
    serviceNames.add(service.name);

    if (service.upstreams.length === 0) {
      issues.push(`Service #${index + 1}: ต้องมี upstream อย่างน้อย 1 รายการ`);
    }
    if (service.circuit_breaker.enabled) {
      if (service.circuit_breaker.consecutive_failures <= 0) {
        issues.push(
          `Service #${index + 1}: circuit_breaker.consecutive_failures ต้องมากกว่า 0`
        );
      }
      if (service.circuit_breaker.open_ms <= 0) {
        issues.push(`Service #${index + 1}: circuit_breaker.open_ms ต้องมากกว่า 0`);
      }
    }
    service.upstreams.forEach((upstream, uidx) => {
      if (!upstream.addr.trim()) {
        issues.push(`Service #${index + 1} Upstream #${uidx + 1}: addr ห้ามว่าง`);
      }
      if (upstream.weight < 1 || upstream.weight > 256) {
        issues.push(`Service #${index + 1} Upstream #${uidx + 1}: weight ต้องอยู่ในช่วง 1..256`);
      }
    });
  });

  duplicateServiceNames.forEach((name) => {
    issues.push(`ชื่อ service "${name}" ซ้ำกัน`);
  });

  const defaultCount = $config.routes.filter((route) => route.is_default).length;
  if (defaultCount > 1) {
    issues.push('กำหนด route ที่เป็น default ได้เพียง 1 รายการ');
  }

  $config.routes.forEach((route, index) => {
    if (!route.path_prefix.startsWith('/')) {
      issues.push(`Route #${index + 1}: path_prefix ต้องขึ้นต้นด้วย /`);
    }
    if (!serviceNames.has(route.service)) {
      issues.push(`Route #${index + 1}: อ้างอิง service "${route.service}" ไม่พบ`);
    }
  });

  return issues;
});

export const resetConfig = (): void => {
  configStore.set(createDefaultConfig());
};

export const addService = (): void => {
  configStore.update((config) => {
    const idx = config.services.length + 1;
    config.services.push(createDefaultService(idx));
    return config;
  });
};

export const removeService = (index: number): void => {
  configStore.update((config) => {
    const removedName = config.services[index]?.name ?? '';
    config.services.splice(index, 1);
    if (config.services.length === 0) {
      config.services.push(createDefaultService(1));
    }
    config.routes = config.routes.filter((r) => r.service !== removedName);
    return config;
  });
};

export const addServiceUpstream = (serviceIndex: number): void => {
  configStore.update((config) => {
    config.services[serviceIndex]?.upstreams.push(createDefaultUpstream());
    return config;
  });
};

export const removeServiceUpstream = (serviceIndex: number, upstreamIndex: number): void => {
  configStore.update((config) => {
    const service = config.services[serviceIndex];
    if (!service) {
      return config;
    }
    service.upstreams.splice(upstreamIndex, 1);
    if (service.upstreams.length === 0) {
      service.upstreams.push(createDefaultUpstream());
    }
    return config;
  });
};

export const addRoute = (): void => {
  configStore.update((config) => {
    const idx = config.routes.length + 1;
    const firstServiceName = config.services[0]?.name ?? 'service-1';
    config.routes.push(createDefaultRoute(idx, firstServiceName));
    return config;
  });
};

export const removeRoute = (index: number): void => {
  configStore.update((config) => {
    config.routes.splice(index, 1);
    if (config.routes.length === 0) {
      const firstServiceName = config.services[0]?.name ?? 'service-1';
      config.routes.push(createDefaultRoute(1, firstServiceName));
    }
    return config;
  });
};