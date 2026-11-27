const STEP = 200;

/** How many table rows to render. Grows on "show more", or when the cursor moves past the end. */
export class Paging {
	#extra = $state(0);
	#cursor: () => number;

	constructor(cursor: () => number) {
		this.#cursor = cursor;
	}

	get limit() {
		const needed = Math.ceil((this.#cursor() + 1) / STEP) * STEP;
		return Math.max(STEP + this.#extra, needed);
	}

	more() {
		this.#extra = this.limit;
	}
}
