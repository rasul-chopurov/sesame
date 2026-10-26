<script setup>
// Product logos from the desktop app's assets (simple-icons, CC0).
defineProps({ name: String, size: { type: Number, default: 16 } })
</script>

<template>
  <img v-if="name" :src="`/brands/${name}.svg`" :width="size" :height="size" alt="" style="flex:none" @error="$event.target.style.display = 'none'">
</template>
