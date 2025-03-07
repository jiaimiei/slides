<script lang="ts">
	import { Button } from "$lib/components/ui/button"
	import FileVideo from "lucide-svelte/icons/file-video"
	import Settings from "lucide-svelte/icons/settings"
	import { open } from "@tauri-apps/api/dialog"
	import { session } from "$lib/session"
	import { goto } from "$app/navigation"
	import * as Alert from "$lib/components/ui/alert"
	import CloudDownload from "lucide-svelte/icons/cloud-download"
	import { checkUpdate, installUpdate, type UpdateManifest } from "@tauri-apps/api/updater"
	import { relaunch } from "@tauri-apps/api/process"
	import { open as shellOpen } from "@tauri-apps/api/shell"
	import { onMount } from "svelte"
	import Download from "lucide-svelte/icons/download"
	import { getVersion } from "@tauri-apps/api/app"

	let updateManifest: UpdateManifest | undefined = undefined
	let installingUpdate = false

	onMount(() => {
		;(async () => {
			const { shouldUpdate, manifest } = await checkUpdate()

			if (shouldUpdate) {
				updateManifest = manifest
			}
		})()
	})
</script>

<main class="w-screen h-screen bg-muted/40 p-16 xl:p-64">
	<h1 class="text-9xl font-extrabold tracking-tight mb-4">Slides</h1>
	<div class="flex flex-wrap gap-2">
		<Button
			on:click={async () => {
				const videoPath = await open({
					title: "Select a lecture recording",
					filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv"] }]
				})

				if (typeof videoPath === "string") {
					session.videoPath = videoPath
					goto("/lecture")
				}
			}}><FileVideo class="mr-2 h-4 w-4" /> Select a lecture recording</Button
		>
		<Button href="/settings"><Settings class="mr-2 h-4 w-4" /> Manage settings</Button>
	</div>
	{#if updateManifest}
		<Alert.Root class="mt-4">
			<CloudDownload class="h-4 w-4" />
			<Alert.Title>Update available to v{updateManifest.version} (current: {#await getVersion() then x}{x}{/await})</Alert.Title>
			<Alert.Description>
				<div>
					A changelog is available <Button class="p-0 h-0" variant="link" on:click={() => shellOpen("https://github.com/jiaimiei/slides/releases")}>here</Button>.
				</div>
				<Button
					class="mt-4"
					size="sm"
					disabled={installingUpdate}
					on:click={async () => {
						installingUpdate = true
						await installUpdate()
						await relaunch()
					}}><Download class="mr-2 h-4 w-4" /> Install</Button
				>
			</Alert.Description>
		</Alert.Root>
	{/if}
</main>
