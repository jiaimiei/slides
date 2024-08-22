<script lang="ts">
	import { onDestroy, onMount } from "svelte"
	import { invoke, convertFileSrc } from "@tauri-apps/api/tauri"
	import { listen } from "@tauri-apps/api/event"
	import { session } from "$lib/session"
	import { Progress } from "$lib/components/ui/progress"
	import { readTextFile } from "@tauri-apps/api/fs"
	import { join } from "@tauri-apps/api/path"

	const unlisten = { run: () => {} }

	let progress: {
		transcoding: "started" | "done" | null
		transcribing: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } | null
		processing: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } | null
		gatheringPreviews: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } | null
		finalising: "started" | "done" | null
	} = {
		transcoding: null,
		transcribing: null,
		processing: null,
		gatheringPreviews: null,
		finalising: null
	}

	let dataPath = ""

	let data: {
		segments: { text: string; start: number; end: number }[]
		words: { text: string; start: number; end: number }[]
		start: number
		end: number
	}[] = null!

	let currentTime = 0

	let video: HTMLVideoElement

	onMount(async () => {
		const unlisten1 = await listen<
			| { type: "transcoding"; data: "started" | "done" }
			| { type: "transcribing"; data: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } }
			| { type: "processing"; data: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } }
			| { type: "gatheringPreviews"; data: { type: "preparing" } | { type: "progress"; data: number } | { type: "done" } }
			| { type: "finalising"; data: "started" | "done" }
		>("progress", (evt) => {
			// @ts-expect-error
			progress[evt.payload.type] = evt.payload.data
		})

		const unlisten2 = await listen<string>("complete", async (evt) => {
			dataPath = evt.payload
			data = JSON.parse(await readTextFile(await join(evt.payload, "regions.json")))
		})

		unlisten.run = () => {
			unlisten1()
			unlisten2()
		}

		void invoke("rs_process_regions", { videoPath: session.videoPath })
	})

	onDestroy(() => {
		unlisten.run()
	})
</script>

{#if data}
	<main class="w-screen h-screen bg-muted/40 p-8 xl:p-16">
		<div class="grid grid-cols-4 xl:grid-cols-5 gap-4 h-full">
			<div class="flex flex-col basis-0 h-full">
				<h1 class="text-4xl font-extrabold tracking-tight mb-4">Slides</h1>
				<div class="flex-grow basis-0 flex flex-col gap-4 pr-2 overflow-y-auto">
					{#each data.entries() as [idx, { start, end }]}
						{#await (async () => convertFileSrc(await join(dataPath, `${idx}.png`)))() then src}
							<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
							<img
								{src}
								alt=""
								class={`${currentTime >= start && currentTime < end ? "border-blue-600 border-2 p-[4px]" : "p-[6px]"} hover:p-[4px] hover:border-2 hover:border-blue-400 cursor-pointer`}
								on:click={() => {
									video.currentTime = start
								}}
							/>
						{/await}
					{/each}
				</div>
			</div>

			<div class="col-span-3 xl:col-span-4">
				<h1 class="text-4xl font-extrabold tracking-tight mb-4">Video</h1>
				<!-- svelte-ignore a11y-media-has-caption -->
				<div class="flex gap-4">
					<video
						class="h-[50vh]"
						src={convertFileSrc(session.videoPath)}
						controls
						bind:this={video}
						on:timeupdate={() => {
							currentTime = video.currentTime
						}}
					/>
					<div class="text-lg">
						{#each data.find((a) => currentTime >= a.start && currentTime < a.end)?.segments.map((a) => a.text) || [] as segment}
							<div>{segment}</div>
						{/each}
					</div>
				</div>
				<div class="mt-4 text-xl">
					{data
						.find((a) => currentTime >= a.start && currentTime < a.end)
						?.words.map((a) => a.text)
						.join("")}
				</div>
			</div>
		</div>
	</main>
{:else}
	<main class="w-screen h-screen bg-muted/40 p-16 xl:p-32">
		<h1 class="text-9xl font-extrabold tracking-tight mb-4">Slides</h1>
		<div class="text-2xl font-semibold tracking-tight mb-2">Preparing lecture</div>
		<div class="flex flex-col gap-2 xl:w-[40vw]">
			{#if progress.transcoding}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Transcoding</span>
					<div class="col-span-2">
						{progress.transcoding === "started" ? "In progress" : "Done!"}
					</div>
				</div>
			{/if}
			{#if progress.transcribing}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Transcribing</span>
					<div class="col-span-2">
						{#if progress.transcribing.type === "preparing"}
							Preparing
						{:else if progress.transcribing.type === "progress"}
							<Progress max={1} value={progress.transcribing.data} />
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
			{#if progress.processing}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Extracting slides</span>
					<div class="col-span-2">
						{#if progress.processing.type === "preparing"}
							Preparing
						{:else if progress.processing.type === "progress"}
							<Progress max={1} value={progress.processing.data} />
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
			{#if progress.gatheringPreviews}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Gathering previews</span>
					<div class="col-span-2">
						{#if progress.gatheringPreviews.type === "preparing"}
							Preparing
						{:else if progress.gatheringPreviews.type === "progress"}
							<Progress max={1} value={progress.gatheringPreviews.data} />
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
			{#if progress.finalising}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Finalising</span>
					<div class="col-span-2">
						{progress.finalising === "started" ? "In progress" : "Done!"}
					</div>
				</div>
			{/if}
		</div>
	</main>
{/if}
