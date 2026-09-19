<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  let { delayDuration = 200, ...restProps }: TooltipPrimitive.RootProps = $props();
</script>

<!-- bits-ui requires a provider above every tooltip; carrying one here means a
     component can own a tooltip without the page having to remember to. -->
<TooltipPrimitive.Provider {delayDuration}>
  <TooltipPrimitive.Root {...restProps} />
</TooltipPrimitive.Provider>
