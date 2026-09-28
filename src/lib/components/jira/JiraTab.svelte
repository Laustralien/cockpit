<script lang="ts">
  import JiraView from "./JiraView.svelte";
  let { name }: { name: string } = $props();
</script>

<JiraView projet={name} />
