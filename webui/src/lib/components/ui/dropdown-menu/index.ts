import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';

import CheckboxItem from './dropdown-menu-checkbox-item.svelte';
import Content from './dropdown-menu-content.svelte';
import Item from './dropdown-menu-item.svelte';
import Label from './dropdown-menu-label.svelte';
import RadioItem from './dropdown-menu-radio-item.svelte';
import Separator from './dropdown-menu-separator.svelte';
import Shortcut from './dropdown-menu-shortcut.svelte';

const Root = DropdownMenuPrimitive.Root;
const Trigger = DropdownMenuPrimitive.Trigger;
const Group = DropdownMenuPrimitive.Group;
const RadioGroup = DropdownMenuPrimitive.RadioGroup;

export {
  Root,
  Trigger,
  Group,
  RadioGroup,
  CheckboxItem,
  Content,
  Item,
  Label,
  RadioItem,
  Separator,
  Shortcut,
  Root as DropdownMenu,
  Trigger as DropdownMenuTrigger,
  Group as DropdownMenuGroup,
  RadioGroup as DropdownMenuRadioGroup,
  RadioItem as DropdownMenuRadioItem,
  CheckboxItem as DropdownMenuCheckboxItem,
  Content as DropdownMenuContent,
  Item as DropdownMenuItem,
  Label as DropdownMenuLabel,
  Separator as DropdownMenuSeparator,
  Shortcut as DropdownMenuShortcut
};
