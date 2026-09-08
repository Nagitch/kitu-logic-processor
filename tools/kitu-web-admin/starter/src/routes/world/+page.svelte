<script lang="ts">
  import { WorldPage } from '@kitu/admin/pages'
  const kinds = [
    { value: 'object', label: 'Object' },
    { value: 'marker', label: 'Marker' }
  ]
</script>

<WorldPage {kinds} />
