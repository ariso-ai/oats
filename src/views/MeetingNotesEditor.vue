<script setup lang="ts">
import { watch, onBeforeUnmount } from 'vue';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Typography from '@tiptap/extension-typography';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Markdown } from 'tiptap-markdown';

// Compact TipTap surface for in-meeting notes. It speaks Markdown at the
// boundary so local `note.md` files and future backend notes share one format.
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
    Markdown.configure({
      html: false,
      transformPastedText: true,
      transformCopiedText: true,
    }),
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
    // local `note.md` artifact and Agents' existing Tiptap markdown convention.
    emit('update:modelValue', ed.storage.markdown.getMarkdown());
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
    if (!ed || value === ed.storage.markdown.getMarkdown()) return;
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
  padding: 28px 32px 44px;
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

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"]) {
  list-style: none;
  padding-left: 0;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror ul[data-type="taskList"] li) {
  display: flex;
  gap: 8px;
}

.meeting-notes-editor :deep(.meeting-notes-prosemirror blockquote) {
  margin: 12px 0;
  padding-left: 12px;
  border-left: 3px solid #d1d5db;
  color: #4b5563;
}
</style>
