<script lang="ts">
	import { Button } from "$lib/components/ui/button"
	import FileVideo from "lucide-svelte/icons/file-video"
	import { open } from "@tauri-apps/api/dialog"
	import { session } from "$lib/session"
	import { goto } from "$app/navigation"
</script>

<main class="w-screen h-screen bg-muted/40 p-16 xl:p-64">
	<h1 class="text-9xl font-extrabold tracking-tight mb-4">Slides</h1>
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
</main>
