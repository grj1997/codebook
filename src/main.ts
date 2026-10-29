import { createApp } from 'vue';
import App from './App.vue';
import Picker from './Picker.vue';
import './style.css';

const picker = new URLSearchParams(location.search).get('window') === 'picker';
createApp(picker ? Picker : App).mount('#app');
