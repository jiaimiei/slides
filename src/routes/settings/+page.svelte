<script lang="ts">
	import { Button } from "$lib/components/ui/button"
	import { Input } from "$lib/components/ui/input"
	import FileVideo from "lucide-svelte/icons/file-video"
	import { open } from "@tauri-apps/api/dialog"
	import { session } from "$lib/session"
	import { goto } from "$app/navigation"
	import { rsGetSettings, rsSaveSettings, type AppSettings } from "$lib/bindings"
	import { onMount } from "svelte"
	import { Checkbox } from "$lib/components/ui/checkbox"
	import { Label } from "$lib/components/ui/label"
	import { Textarea } from "$lib/components/ui/textarea"
	import ArrowLeft from "lucide-svelte/icons/arrow-left"

	let settings: AppSettings | null = null

	onMount(async () => {
		settings = await rsGetSettings()
	})

	$: if (settings) rsSaveSettings(settings)
</script>

<main class="w-screen h-screen bg-muted/40 p-16 xl:p-64">
	<h1 class="text-9xl font-extrabold tracking-tight mb-4">Settings</h1>
	<Button href="/"><ArrowLeft class="mr-2 h-4 w-4" /> Back</Button>

	{#if settings}
		<div class="mt-8 items-top flex space-x-2">
			<Checkbox id="useAI" bind:checked={settings.ai.use_ai} />
			<div class="grid gap-1.5 leading-none">
				<Label for="useAI" class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">Use AI slide summaries</Label>
				<p class="text-muted-foreground text-sm">You can use an AI service to automatically create summaries from slide transcripts.</p>
			</div>
		</div>
		{#if settings.ai.use_ai}
			<h2 class="text-xl font-semibold mt-4">AI settings</h2>
			<div class="mt-2 grid w-full max-w-md items-center gap-1.5">
				<Label for="baseURL">Base URL</Label>
				<Input type="url" id="baseURL" placeholder="https://api.mistral.ai/v1" bind:value={settings.ai.base_url} />
				<p class="text-muted-foreground text-sm">The URL of the AI service's API; must be OpenAI-compatible.</p>
			</div>
			<div class="mt-4 grid w-full max-w-md items-center gap-1.5">
				<Label for="apiKey">API key</Label>
				<Input type="password" id="apiKey" placeholder="Your API key" bind:value={settings.ai.key} />
			</div>
			<div class="mt-4 grid w-full max-w-md items-center gap-1.5">
				<Label for="model">Model</Label>
				<Input type="text" id="model" placeholder="mistral-large-latest" bind:value={settings.ai.model} />
			</div>
			<div class="mt-4 grid w-full gap-1.5 max-w-lg">
				<Label for="promptTemplate">Prompt template</Label>
				<Textarea
					placeholder={"The following is an excerpt from a lecture transcript:\n\n##text##\n\nReformat this excerpt in paragraphed, readable form. Correct any spelling or grammar issues. Give only the reformatted text in your response."}
					id="promptTemplate"
					bind:value={settings.ai.prompt_template}
				/>
				<p class="text-muted-foreground text-sm">
					This will be passed to the AI model to create the Slide Summary. Wherever you include ##text##, it will be replaced with the original slide transcript. Get creative - you could use
					this to create a summary, a quiz, or even a full set of notes! The default template is a simple reformatting of the text to improve readability.
				</p>
			</div>
		{/if}
	{/if}
</main>
