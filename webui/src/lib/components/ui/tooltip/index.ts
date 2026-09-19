import { Tooltip as TooltipPrimitive } from 'bits-ui';

import Root from './tooltip.svelte';
import Content from './tooltip-content.svelte';

const Trigger = TooltipPrimitive.Trigger;
const Provider = TooltipPrimitive.Provider;

export {
  Root,
  Trigger,
  Provider,
  Content,
  Root as Tooltip,
  Trigger as TooltipTrigger,
  Provider as TooltipProvider,
  Content as TooltipContent
};
