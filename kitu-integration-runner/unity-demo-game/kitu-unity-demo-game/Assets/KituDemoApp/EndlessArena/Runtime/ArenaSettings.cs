using UnityEngine;

namespace UnityOnlyArena
{
    public sealed class ArenaSettings
    {
        public const string PreferencePrefix = "UnityOnlyArena.Settings.";
        public float Volume = 1f;
        public bool Fullscreen;

        public ArenaSettings Copy() => new ArenaSettings { Volume = Volume, Fullscreen = Fullscreen };

        public static ArenaSettings Load(string prefix = PreferencePrefix)
        {
            float volume = PlayerPrefs.GetFloat(prefix + "Volume", 1f);
            if (float.IsNaN(volume) || float.IsInfinity(volume) || volume < 0f || volume > 1f)
                volume = 1f;
            return new ArenaSettings { Volume = volume, Fullscreen = PlayerPrefs.GetInt(prefix + "Fullscreen", 0) == 1 };
        }

        public void Apply(bool save = true, string prefix = PreferencePrefix)
        {
            Volume = float.IsNaN(Volume) || float.IsInfinity(Volume) ? 1f : Mathf.Clamp01(Volume);
            AudioListener.volume = Volume;
#if !UNITY_EDITOR
            if (Fullscreen)
                Screen.SetResolution(Screen.currentResolution.width, Screen.currentResolution.height, FullScreenMode.FullScreenWindow);
            else
                Screen.SetResolution(1920, 1200, FullScreenMode.Windowed);
#endif
            if (!save) return;
            PlayerPrefs.SetFloat(prefix + "Volume", Volume);
            PlayerPrefs.SetInt(prefix + "Fullscreen", Fullscreen ? 1 : 0);
            PlayerPrefs.Save();
        }
    }
}
