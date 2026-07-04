<script setup lang="ts">
import { watch, onBeforeUnmount } from 'vue';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Typography from '@tiptap/extension-typography';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Markdown } from '@tiptap/markdown';

// Compact TipTap surface for in-meeting notes. It speaks Markdown at the
// boundary so local `user-note.md` files and future backend notes share one format.
const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    /** Controls whether the ProseMirror surface accepts input while parent
     * panes load or save the meeting artifact. */
    editable?: boolean;
  }>(),
  {
    placeholder: '',
    editable: true,
  }
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  blur: [];
}>();

const editor = useEditor({
  extensions: [
    StarterKit.configure({ heading: { levels: [2, 3] } }),
    Markdown,
    Placeholder.configure({ placeholder: props.placeholder }),
    Typography,
    TaskList,
    TaskItem.configure({ nested: true }),
  ],
  editable: props.editable,
  content: props.modelValue || '',
  editorProps: {
    attributes: {
      class: 'meeting-notes-prosemirror',
    },
  },
  onUpdate: ({ editor: ed }) => {
    // Emit markdown rather than HTML so the editor stays compatible with the
    // local `user-note.md` artifact and Agents' existing Tiptap markdown convention.
    emit('update:modelValue', ed.getMarkdown());
  },
  onCreate: ({ editor: ed }) => {
    if (props.modelValue) {
      ed.commands.setContent(props.modelValue, { contentType: 'markdown' });
    }
  },
  onBlur: () => emit('blur'),
});

watch(
  () => props.modelValue,
  (value) => {
    const ed = editor.value;
    if (!ed || value === ed.getMarkdown()) return;
    ed.commands.setContent(value || '', { contentType: 'markdown' });
  }
);

watch(
  () => props.editable,
  (editable) => {
    editor.value?.setEditable(editable);
  }
);

onBeforeUnmount(() => {
  editor.value?.destroy();
});
</script>

<template>
  <EditorContent v-if="editor" class="meeting-notes-editor" :editor="editor" />
</template>

<style scoped>
.meeting-notes-editor {
  flex: 1;
  min-height: 0;
  display: flex;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror) {
  flex: 1;
  min-height: 100%;
  outline: none;
  padding: 18px 0 44px;
  font: 15px/1.58 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
  color: #1d1d1f;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror p.is-editor-empty:first-child::before) {
  content: attr(data-placeholder);
  color: #9ca3af;
  float: left;
  height: 0;
  pointer-events: none;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror h2) {
  margin: 22px 0 8px;
  font-size: 17px;
  line-height: 1.3;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror h3) {
  margin: 18px 0 6px;
  font-size: 15px;
  line-height: 1.35;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror p) {
  margin: 7px 0;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul),
.meeting-notes-editor :deep(.meeting-notes-prosemirror ol) {
  margin: 8px 0;
  padding-left: 24px;
}

/* Tailwind's preflight resets list markers to `none`; restore them so the
   editing surface shows bullets and numbers as the user types. */
.meeting-notes-editor :deep(.meeting-notes-prosemirror ul) {
  list-style-type: disc;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ol) {
  list-style-type: decimal;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror li) {
  display: list-item;
  list-style-position: outside;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"]) {
  list-style: none;
  padding-left: 0;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"] li) {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

/* Keep the checkbox inline with the first line of its text: pin the label,
   nudge it to the text's optical center, and drop the paragraph's block
   margin so the item stays a single tight line. */
.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"] li > label) {
  flex: 0 0 auto;
  margin-top: 0;
  user-select: none;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"] li > div) {
  flex: 1 1 auto;
  min-width: 0;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"] li > div > p) {
  margin: 0;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror blockquote) {
  margin: 12px 0;
  padding-left: 12px;
  border-left: 3px solid #d1d5db;
  color: #4b5563;
}
</style>
