<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let name = $state("");
  let greetMsg = $state("");

  async function greet(event: Event) {
    event.preventDefault();
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    greetMsg = await invoke("greet", { name });
  }
</script>

<main class="container">
  <h1>FCH Client</h1>
  <p>This is a modular skeleton. Use the left sidebar to navigate sections.</p>
  <div class="grid">
    <div class="card">Responsive layout</div>
    <div class="card">Grayscale theme</div>
    <div class="card">Collapsible sidebar</div>
    <div class="card">Six placeholder buttons</div>
  </div>
</main>

<style>
.container { margin: 0; padding: 8px; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin-top: 16px; }
.card { padding: 16px; border: 1px solid var(--border); background: var(--bg-elev); color: var(--fg); border-radius: 12px; text-align: center; }
h1 { text-align: center; }
</style>
