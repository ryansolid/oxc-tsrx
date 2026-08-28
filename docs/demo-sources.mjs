// The canonical TSRX snippets the demo surfaces use. They live here, in one
// module, because the clickable examples derive variants from them by string
// replacement: when a snippet and its anchor drift apart the derived variant
// silently becomes a no-op, which is exactly how the hero's "Type-aware lint"
// example went dead. The assertions below turn that drift into a build error.

// Real TSRX hero snippet, highlighted with the actual TSRX grammar. This is
// oxc-tsrx-fmt's converged output, so the default demo state is format-clean.
export const heroCode = `export function TaskList({ tasks }: Props) @{
  const pending = tasks.filter((task) => !task.done);

  <section class="tasks">
    @if (pending.length > 0) {
      @for (const task of pending; key task.id) {
        <TaskRow task={task} />;
      } @empty {
        <AllDone />;
      }
    } @else {
      <SignIn />;
    }
    <style>
      .tasks { display: grid; gap: 0.5rem; }
    </style>
  </section>;
}`

// The "Type-aware lint" example. The point is a finding only tsgolint can
// reach: oxlint alone sees `saveTask(task);` as an ordinary call, and only
// type information reveals it returns a Promise nobody awaits. A plain
// compiler error (a misspelled property, say) would prove nothing here, since
// any editor's TypeScript server already reports those.
//
// Self-contained on purpose: it declares its own Task and saveTask so the run
// produces this one finding instead of a pile of "Cannot find name".
export const typeAwareCode = `type Task = { id: string; label: string; done: boolean };

async function saveTask(task: Task): Promise<void> {
  await Promise.resolve(task.id);
}

export function TaskRow({ task }: { task: Task }) @{
  function toggle() {
    saveTask(task);
  }

  <li>
    <button onClick={toggle}>{task.label}</button>
  </li>;
}`

// The unawaited call is the whole example; without it the run comes back clean
// and the chip goes quiet again.
export const TYPE_AWARE_ANCHOR = 'saveTask(task);'
if (!typeAwareCode.includes(TYPE_AWARE_ANCHOR)) {
  throw new Error('demo-sources: typeAwareCode no longer contains the unawaited call')
}

// The lint scenario derives its variant from this anchor; assert it so the
// "Lint findings" and "Custom config" examples cannot go quiet either.
const LINT_ANCHOR = 'const pending = tasks.filter((task) => !task.done);'
for (const [name, snippet] of Object.entries({ heroCode })) {
  if (!snippet.includes(LINT_ANCHOR)) {
    throw new Error(`demo-sources: ${name} no longer contains the lint example anchor`)
  }
}
