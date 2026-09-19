import { Select as SelectPrimitive } from 'bits-ui';

import Content from './select-content.svelte';
import Item from './select-item.svelte';
import Label from './select-label.svelte';
import Separator from './select-separator.svelte';
import Trigger from './select-trigger.svelte';

const Root = SelectPrimitive.Root;
const Group = SelectPrimitive.Group;

export {
  Root,
  Group,
  Content,
  Item,
  Label,
  Separator,
  Trigger,
  Root as Select,
  Group as SelectGroup,
  Content as SelectContent,
  Item as SelectItem,
  Label as SelectLabel,
  Separator as SelectSeparator,
  Trigger as SelectTrigger
};
