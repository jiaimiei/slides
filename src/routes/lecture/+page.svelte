<script lang="ts">
	import { onDestroy, onMount } from "svelte"
	import { invoke, convertFileSrc } from "@tauri-apps/api/tauri"
	import { listen } from "@tauri-apps/api/event"
	import { session } from "$lib/session"
	import { Progress } from "$lib/components/ui/progress"
	import { readTextFile, exists } from "@tauri-apps/api/fs"
	import { join } from "@tauri-apps/api/path"
	import { Badge } from "$lib/components/ui/badge"
	import scrollIntoView from "scroll-into-view-if-needed"
	import { platform } from "@tauri-apps/api/os"
	import DOMPurify from "dompurify"
	import { marked } from "marked"

	const unlisten = { run: () => {} }

	let progress: {
		downloading: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } | null
		transcoding: "started" | "done" | null
		transcribing: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } | null
		processing: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } | null
		gatheringPreviews: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } | null
		summarising: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } | null
	} = {
		downloading: null,
		transcoding: null,
		transcribing: null,
		processing: null,
		gatheringPreviews: null,
		summarising: null
	}

	let serverSecret = ""
	let dataPath = ""

	let data: {
		segments: { text: string; start: number; end: number }[]
		words: { text: string; start: number; end: number }[] | null
		start: number
		end: number
		summary: string
	}[] = null!

	let currentTime = 0

	let video: HTMLVideoElement

	let error: string | null = null

	onMount(async () => {
		const unlisten1 = await listen<
			| { type: "downloading"; data: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } }
			| { type: "transcoding"; data: "started" | "done" }
			| { type: "transcribing"; data: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } }
			| { type: "processing"; data: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } }
			| { type: "gatheringPreviews"; data: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } }
			| { type: "summarising"; data: { type: "preparing" } | { type: "progress"; data: [number, number] } | { type: "done" } }
		>("progress", (evt) => {
			// @ts-expect-error
			progress[evt.payload.type] = evt.payload.data
		})

		const unlisten2 = await listen<string>("complete", async (evt) => {
			;[serverSecret, dataPath] = evt.payload
			data = JSON.parse(await readTextFile(await join(dataPath, "regions.json")))
		})

		unlisten.run = () => {
			unlisten1()
			unlisten2()
		}

		void invoke("rs_process_regions", { videoPath: session.videoPath }).catch((err) => {
			error = String(err)
		})
	})

	onDestroy(() => {
		unlisten.run()
	})

	function secondsToTime(s: number) {
		const date = new Date(0)
		date.setSeconds(s)
		return date.toISOString().substring(11, 19).startsWith("00") ? date.toISOString().substring(14, 19) : date.toISOString().substring(11, 19)
	}

	$: if (data) {
		const currentSegmentTime = data.flatMap((a) => a.segments).find((a) => currentTime >= a.start && currentTime < a.end)?.start

		if (currentSegmentTime) {
			let elem = document.getElementById(`segment-${secondsToTime(currentSegmentTime)}`)

			if (elem) {
				scrollIntoView(elem, { scrollMode: "if-needed" })
			}
		}

		const currentSlideTime = data.find((a) => currentTime >= a.start && currentTime < a.end)?.start

		if (currentSlideTime) {
			let elem = document.getElementById(`slide-${secondsToTime(currentSlideTime)}`)

			if (elem) {
				scrollIntoView(elem, { scrollMode: "if-needed" })
			}
		}
	}

	// Split a list of segments into their tokens.
	function splitSegments(
		segments: { text: string; start: number; end: number }[],
		tokens: ({ text: string; start: number; end: number } | null)[]
	): [{ text: string; start: number; end: number }, { text: string; start: number; end: number }[]][] {
		if (tokens.length && tokens.every((a) => a)) {
			const ts: { text: string; start: number; end: number }[] = tokens.slice(
				tokens.findIndex((token) => segments[0].text.startsWith(token!.text)),
				tokens.findLastIndex((token) => segments.at(-1)?.text.endsWith(token!.text)) + 1
			) as any

			let curToken = 0

			let splitSegments: [{ text: string; start: number; end: number }, { text: string; start: number; end: number }[]][] = []

			for (const segment of segments) {
				let i = 0

				let split = []

				while (i < segment.text.length) {
					i += ts[curToken].text.length
					split.push(ts[curToken])

					curToken += 1
				}

				splitSegments.push([segment, split])
			}

			return splitSegments
		} else {
			return segments.map((a) => [a, []])
		}
	}
</script>

<svelte:window
	on:keydown={(evt) => {
		if (video && evt.target !== video) {
			if (evt.key === " ") {
				if (video.paused) {
					video.play()
				} else {
					video.pause()
				}

				evt.preventDefault()
			} else if (evt.key === "ArrowLeft") {
				video.currentTime -= 5

				evt.preventDefault()
			} else if (evt.key === "ArrowRight") {
				video.currentTime += 5

				evt.preventDefault()
			}
		}
	}}
/>

{#if data}
	<main class="w-screen h-screen bg-muted/40 p-8 xl:p-16">
		<div class="grid grid-cols-4 xl:grid-cols-5 gap-4 h-full">
			<div class="flex flex-col basis-0 h-full">
				<h1 class="text-4xl font-extrabold tracking-tight mb-4">Slides</h1>
				<div class="flex-grow basis-0 flex flex-col gap-4 pr-2 overflow-y-auto">
					{#each data.entries() as [idx, { start, end }]}
						{#await (async () => convertFileSrc(await join(dataPath, `${idx}.png`)))() then src}
							<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
							<div id="slide-{secondsToTime(start)}" class="relative">
								<img
									{src}
									alt=""
									class={`${currentTime >= start && currentTime < end ? "border-blue-600 border-2 p-[4px]" : "p-[6px]"} hover:p-[4px] hover:border-2 hover:border-blue-400 cursor-pointer`}
									on:click={() => {
										video.currentTime = start
									}}
								/>
								<Badge class="absolute bottom-4 right-4">{secondsToTime(start)} - {secondsToTime(end)}</Badge>
								<Badge class="absolute bottom-4 left-4">{idx + 1}</Badge>
							</div>
						{/await}
					{/each}
				</div>
			</div>

			<div class="col-span-3 xl:col-span-4 flex flex-col">
				<h1 class="text-4xl font-extrabold tracking-tight mb-4">Video</h1>
				<!-- svelte-ignore a11y-media-has-caption -->
				<div class="flex gap-4 h-[50vh]">
					{#await platform() then platform}
						<video
							src={platform === "win32" ? convertFileSrc(session.videoPath) : `http://localhost:52937/${serverSecret}`}
							controls
							bind:this={video}
							on:timeupdate={() => {
								currentTime = video.currentTime
							}}
							on:loadedmetadata={async () => {
								if (await exists(await join(dataPath, "current_time.txt"))) {
									video.currentTime = Number(await readTextFile(await join(dataPath, "current_time.txt")))
								} else {
									video.currentTime = data[0].segments[0].start
								}

								setInterval(async () => {
									if (video) {
										await invoke("save_current_time", { dataPath, time: video.currentTime })
									}
								}, 2000)
							}}
							class="outline-none"
						/>
					{/await}
					<div class="text-base h-full overflow-y-auto pr-2">
						{#if true}
							{@const dedupSegments = data
								.flatMap((a) => a.segments)
								.filter((i, idx, arr) => arr[idx - 1]?.text !== i.text || arr[idx - 1]?.start !== i.start || arr[idx - 1]?.end !== i.end)}
							{@const dedupWords = data
								.flatMap((a) => a.words)
								.filter((i, idx, arr) => arr[idx - 1]?.text !== i?.text || arr[idx - 1]?.start !== i?.start || arr[idx - 1]?.end !== i?.end)}
							{@const splitSegs = splitSegments(dedupSegments, dedupWords)}
							{#each splitSegs as [segment, tokens]}
								{#if tokens.length}
									<div
										id="segment-{secondsToTime(segment.start)}"
										class="p-2 grid grid-cols-5 2xl:grid-cols-11 gap-2 hover:bg-muted-foreground/15 cursor-pointer {currentTime >= segment.start && currentTime < segment.end
											? 'bg-muted-foreground/10'
											: ''}"
										on:click={() => {
											video.currentTime = segment.start
										}}
									>
										<Badge variant="secondary" class="justify-center">{secondsToTime(segment.start)}</Badge>
										<div class="col-span-4 2xl:col-span-10">
											{#each tokens as token}
												<span
													class="cursor-pointer"
													on:click|stopPropagation={() => {
														video.currentTime = token.start
													}}>{token.text}</span
												>
											{/each}
										</div>
									</div>
								{:else}
									<div
										id="segment-{secondsToTime(segment.start)}"
										class="p-2 grid grid-cols-5 2xl:grid-cols-11 gap-2 hover:bg-muted-foreground/15 cursor-pointer {currentTime >= segment.start && currentTime < segment.end
											? 'bg-muted-foreground/10'
											: ''}"
										on:click={() => {
											video.currentTime = segment.start
										}}
									>
										<Badge variant="secondary" class="justify-center">{secondsToTime(segment.start)}</Badge>
										<div class="col-span-4 2xl:col-span-10">{segment.text}</div>
									</div>
								{/if}
							{/each}
						{/if}
					</div>
				</div>
				<div class="mt-8 flex-grow flex flex-col">
					<h1 class="text-4xl font-extrabold tracking-tight mb-2">Slide Summary</h1>
					<div class="flex-grow basis-0 overflow-y-auto pr-2 text-xl typographic">
						{#if (data.find((a) => currentTime >= a.start && currentTime < a.end)?.summary || "") == ""}
							<div class="text-muted-foreground">No audio</div>
						{:else}
							{#await marked(data.find((a) => currentTime >= a.start && currentTime < a.end)?.summary || "", { gfm: true, silent: true }) then content}
								{@html DOMPurify.sanitize(content)}
							{/await}
						{/if}
					</div>
				</div>
			</div>
		</div>
	</main>
{:else}
	<main class="w-screen h-screen bg-muted/40 p-16 xl:p-32">
		<h1 class="text-9xl font-extrabold tracking-tight mb-4">Slides</h1>
		<div class="text-2xl font-semibold tracking-tight mb-2">Preparing lecture</div>
		<div class="flex flex-col gap-2 xl:w-[40vw]">
			{#if progress.downloading}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Downloading model</span>
					<div class="col-span-2">
						{#if progress.downloading.type === "preparing"}
							Preparing
						{:else if progress.downloading.type === "progress"}
							<div class="flex gap-4 items-center">
								<div class="flex-grow"><Progress max={1} value={progress.downloading.data[0]} /></div>
								<span class="flex-shrink-0 w-32"
									>{secondsToTime(progress.downloading.data[1]) !== "00:00" ? secondsToTime(progress.downloading.data[1]).replace(/^0([0-9]):/, "$1:") : "-:--"} remaining</span
								>
							</div>
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
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
							<div class="flex gap-4 items-center">
								<div class="flex-grow"><Progress max={1} value={progress.transcribing.data[0]} /></div>
								<span class="flex-shrink-0 w-32"
									>{secondsToTime(progress.transcribing.data[1]) !== "00:00" ? secondsToTime(progress.transcribing.data[1]).replace(/^0([0-9]):/, "$1:") : "-:--"} remaining</span
								>
							</div>
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
							<div class="flex gap-4 items-center">
								<div class="flex-grow"><Progress max={1} value={progress.processing.data[0]} /></div>
								<span class="flex-shrink-0 w-32"
									>{secondsToTime(progress.processing.data[1]) !== "00:00" ? secondsToTime(progress.processing.data[1]).replace(/^0([0-9]):/, "$1:") : "-:--"} remaining</span
								>
							</div>
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
							<div class="flex gap-4 items-center">
								<div class="flex-grow"><Progress max={1} value={progress.gatheringPreviews.data[0]} /></div>
								<span class="flex-shrink-0 w-32"
									>{secondsToTime(progress.gatheringPreviews.data[1]) !== "00:00" ? secondsToTime(progress.gatheringPreviews.data[1]).replace(/^0([0-9]):/, "$1:") : "-:--"} remaining</span
								>
							</div>
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
			{#if progress.summarising}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Summarising</span>
					<div class="col-span-2">
						{#if progress.summarising.type === "preparing"}
							Preparing
						{:else if progress.summarising.type === "progress"}
							<div class="flex gap-4 items-center">
								<div class="flex-grow"><Progress max={1} value={progress.summarising.data[0]} /></div>
								<span class="flex-shrink-0 w-32"
									>{secondsToTime(progress.summarising.data[1]) !== "00:00" ? secondsToTime(progress.summarising.data[1]).replace(/^0([0-9]):/, "$1:") : "-:--"} remaining</span
								>
							</div>
						{:else}
							Done!
						{/if}
					</div>
				</div>
			{/if}
			{#if error}
				<div class="grid grid-cols-3 gap-4 items-center">
					<span class="font-bold">Error</span>
					<div class="col-span-2">
						{error}
					</div>
				</div>
			{/if}
		</div>
	</main>
{/if}
