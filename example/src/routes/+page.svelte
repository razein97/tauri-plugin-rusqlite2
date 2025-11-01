<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Button } from "$lib/components/ui/button";
  import Database from '@razein97/tauri-plugin-rusqlite2';

  
  let results = $state();
  let inputValue = $state("SELECT * FROM accounts;")
  let databaseValue = $state("sqlite:pass:example.db")
  let database = $state() as Database| undefined;
  let extensions = $state('');

  let status = $derived(database === undefined ? 'Disconnected' : 'Connected') as 'Disconnected' | 'Connected';


async function handleConnection(){
  try {
    database = await Database.load(databaseValue, extensions.split(','));
    console.log(database)
  } catch (error) {
    console.error(error)

  }
}

async function execute() {
  results = await database?.select(inputValue);
}

</script>

<main class="w-full h-full flex flex-col items-center space-y-4 p-4">
  <h1 class="font-bold text-center">Rust Plugin Rusqlite2</h1>

  <div class="flex flex-row space-x-2">
    <p>Status:</p>
    <p class={`${status === 'Connected' ? 'text-green-500' : 'text-red-500'}`} >{status}</p>
  </div>

<form class="flex w-full max-w-full items-center space-x-2">
  <Input bind:value={databaseValue} type="text" placeholder="sqlite:pass:db_name" />
  <Input bind:value={extensions}  type="text" placeholder="path/to/ext, path/to/ext" />
  <Button type="submit" onsubmit={async()=>{await handleConnection()}} onclick={async()=>{await handleConnection()}}>Connect</Button>
</form>


<form class="flex w-full max-w-full items-center space-x-2">
  <Input bind:value={inputValue} type="text" placeholder="" />
  <Button type="submit"
  onclick={async()=>{
await execute()
  }}
  >Query</Button>
</form>



<div class="p-2 w-full h-fit border gap-y-2 rounded-md shadow-xs">
<p class="underline font-medium">Results</p>
<p class="text-wrap overflow-auto">
{JSON.stringify(results, null, 2)}
</p>

</div>



</main>

<style>


</style>
